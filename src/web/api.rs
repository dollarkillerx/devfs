use std::collections::HashMap;

use axum::extract::{Multipart, Path, Query, State};
use axum::middleware as axum_mw;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::Deserialize;

use crate::types::{AppState, BucketPermission, BucketPolicy};

use super::require_session;

pub fn routes(state: AppState) -> Router<AppState> {
    // Public routes (no session required)
    let public = Router::new()
        .route("/_web/api/login", post(login));

    // Protected routes (require session)
    let protected = Router::new()
        .route("/_web/api/logout", post(logout))
        .route("/_web/api/buckets", get(list_buckets))
        .route("/_web/api/buckets", post(create_bucket))
        .route("/_web/api/buckets/{name}", delete(delete_bucket))
        .route("/_web/api/buckets/{name}/policy", put(set_bucket_policy))
        .route("/_web/api/buckets/{name}/objects", get(list_objects))
        .route("/_web/api/buckets/{name}/upload", post(upload_object))
        .route("/_web/api/buckets/{name}/download/{*key}", get(download_object))
        .route("/_web/api/buckets/{name}/objects/{*key}", delete(delete_object))
        .route("/_web/api/keys", get(list_keys))
        .route("/_web/api/keys", post(create_key))
        .route("/_web/api/keys/{id}", put(update_key))
        .route("/_web/api/keys/{id}", delete(delete_key))
        .layer(axum_mw::from_fn_with_state(state, require_session));

    public.merge(protected)
}

// ── Login / Logout ──────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Response {
    let Some(ref web_config) = state.web_auth_config else {
        return (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "Web auth not configured"})),
        ).into_response();
    };

    if body.username != web_config.username || body.password != web_config.password {
        return (
            axum::http::StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "Invalid credentials"})),
        ).into_response();
    }

    let token = crate::session::create_token(&body.username, state.jwt_secret.as_bytes());

    Json(serde_json::json!({"ok": true, "token": token})).into_response()
}

async fn logout() -> Response {
    Json(serde_json::json!({"ok": true})).into_response()
}

// ── Buckets ─────────────────────────────────────────────────────────────────

async fn list_buckets(State(state): State<AppState>) -> Response {
    match state.storage.list_buckets().await {
        Ok(buckets) => {
            let ks = state.key_store.read().unwrap();
            let result: Vec<serde_json::Value> = buckets
                .iter()
                .map(|(name, created)| {
                    let policy = ks.get_policy(name);
                    serde_json::json!({
                        "name": name,
                        "created": created,
                        "policy": {
                            "public_read": policy.public_read,
                            "public_write": policy.public_write,
                        }
                    })
                })
                .collect();
            Json(serde_json::json!({"buckets": result})).into_response()
        }
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct CreateBucketRequest {
    name: String,
}

async fn create_bucket(
    State(state): State<AppState>,
    Json(body): Json<CreateBucketRequest>,
) -> Response {
    match state.storage.create_bucket(&body.name).await {
        Ok(()) => Json(serde_json::json!({"ok": true, "name": body.name})).into_response(),
        Err(e) => error_response(e),
    }
}

async fn delete_bucket(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Response {
    match state.storage.delete_bucket(&name).await {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => error_response(e),
    }
}

#[derive(Deserialize)]
struct SetPolicyRequest {
    public_read: bool,
    public_write: bool,
}

async fn set_bucket_policy(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(body): Json<SetPolicyRequest>,
) -> Response {
    // Verify bucket exists
    if let Err(e) = state.storage.head_bucket(&name).await {
        return error_response(e);
    }

    let policy = BucketPolicy {
        public_read: body.public_read,
        public_write: body.public_write,
    };
    state.key_store.write().unwrap().set_policy(&name, policy);
    Json(serde_json::json!({"ok": true})).into_response()
}

// ── Objects ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ListObjectsQuery {
    prefix: Option<String>,
    delimiter: Option<String>,
}

async fn list_objects(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Query(query): Query<ListObjectsQuery>,
) -> Response {
    use crate::types::ListObjectsV2Params;

    let params = ListObjectsV2Params {
        prefix: query.prefix,
        delimiter: query.delimiter,
        max_keys: 1000,
        continuation_token: None,
        start_after: None,
    };

    match state.storage.list_objects_v2(&name, &params).await {
        Ok(result) => {
            let objects: Vec<serde_json::Value> = result
                .objects
                .iter()
                .map(|obj| {
                    serde_json::json!({
                        "key": obj.key,
                        "size": obj.size,
                        "etag": obj.etag,
                        "last_modified": obj.last_modified,
                    })
                })
                .collect();
            Json(serde_json::json!({
                "objects": objects,
                "common_prefixes": result.common_prefixes,
                "is_truncated": result.is_truncated,
            }))
            .into_response()
        }
        Err(e) => error_response(e),
    }
}

async fn upload_object(
    State(state): State<AppState>,
    Path(bucket): Path<String>,
    mut multipart: Multipart,
) -> Response {
    let mut prefix = String::new();
    let mut file_name = None;
    let mut file_data = None;
    let mut content_type = None;

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "prefix" => {
                prefix = field.text().await.unwrap_or_default();
            }
            "file" => {
                file_name = field.file_name().map(|s| s.to_string());
                content_type = field.content_type().map(|s| s.to_string());
                file_data = field.bytes().await.ok();
            }
            _ => {}
        }
    }

    let Some(name) = file_name else {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "No file provided"})),
        ).into_response();
    };

    let Some(data) = file_data else {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Failed to read file data"})),
        ).into_response();
    };

    let key = format!("{}{}", prefix, name);
    let ct = content_type
        .or_else(|| {
            mime_guess::from_path(&name)
                .first()
                .map(|m| m.to_string())
        })
        .unwrap_or_else(|| "application/octet-stream".to_string());

    let size = data.len() as u64;
    let body = axum::body::Body::from(data);

    match state
        .storage
        .put_object_stream(&bucket, &key, body, Some(ct), HashMap::new(), Some(size))
        .await
    {
        Ok(result) => Json(serde_json::json!({
            "ok": true,
            "key": key,
            "etag": result.etag,
            "size": size,
        }))
        .into_response(),
        Err(e) => error_response(e),
    }
}

async fn download_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    let key = key.strip_prefix('/').unwrap_or(&key);
    match state.storage.get_object_stream(&bucket, key).await {
        Ok(result) => {
            let file_name = key.rsplit('/').next().unwrap_or(key);
            let reader = tokio::io::BufReader::new(result.file);
            let stream = tokio_util::io::ReaderStream::new(reader);
            let body = axum::body::Body::from_stream(stream);

            (
                [
                    ("content-type", result.metadata.content_type.as_str()),
                    ("content-disposition", &format!("attachment; filename=\"{}\"", file_name)),
                    ("etag", &result.metadata.etag),
                ],
                body,
            )
                .into_response()
        }
        Err(e) => error_response(e),
    }
}

async fn delete_object(
    State(state): State<AppState>,
    Path((bucket, key)): Path<(String, String)>,
) -> Response {
    let key = key.strip_prefix('/').unwrap_or(&key);
    match state.storage.delete_object(&bucket, key).await {
        Ok(()) => Json(serde_json::json!({"ok": true})).into_response(),
        Err(e) => error_response(e),
    }
}

// ── API Keys ────────────────────────────────────────────────────────────────

async fn list_keys(State(state): State<AppState>) -> Response {
    let ks = state.key_store.read().unwrap();
    let keys: Vec<serde_json::Value> = ks
        .list_keys()
        .iter()
        .map(|k| {
            serde_json::json!({
                "id": k.id,
                "access_key": k.access_key,
                "description": k.description,
                "buckets": k.buckets,
                "created_at": k.created_at,
            })
        })
        .collect();
    Json(serde_json::json!({"keys": keys})).into_response()
}

#[derive(Deserialize)]
struct CreateKeyRequest {
    description: Option<String>,
    #[serde(default)]
    buckets: HashMap<String, BucketPermission>,
}

async fn create_key(
    State(state): State<AppState>,
    Json(body): Json<CreateKeyRequest>,
) -> Response {
    let entry = state.key_store.write().unwrap().create_key(
        body.description.unwrap_or_default(),
        body.buckets,
    );

    // Show secret only on creation
    Json(serde_json::json!({
        "id": entry.id,
        "access_key": entry.access_key,
        "secret_key": entry.secret_key,
        "description": entry.description,
        "buckets": entry.buckets,
        "created_at": entry.created_at,
    }))
    .into_response()
}

#[derive(Deserialize)]
struct UpdateKeyRequest {
    buckets: HashMap<String, BucketPermission>,
    description: Option<String>,
}

async fn update_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateKeyRequest>,
) -> Response {
    let updated = state
        .key_store
        .write()
        .unwrap()
        .update_key_buckets(&id, body.buckets, body.description);

    if updated {
        Json(serde_json::json!({"ok": true})).into_response()
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Key not found"})),
        )
            .into_response()
    }
}

async fn delete_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Response {
    let deleted = state.key_store.write().unwrap().delete_key(&id);
    if deleted {
        Json(serde_json::json!({"ok": true})).into_response()
    } else {
        (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Key not found"})),
        )
            .into_response()
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn error_response(e: crate::error::S3Error) -> Response {
    let status = e.code.status_code();
    (status, Json(serde_json::json!({"error": e.message}))).into_response()
}
