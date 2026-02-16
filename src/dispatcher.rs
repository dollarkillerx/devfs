use axum::extract::{Request, State};
use axum::response::Response;

use crate::error::{S3Error, S3ErrorCode};
use crate::handlers;
use crate::types::{AppState, S3Operation};

pub async fn dispatch(
    State(state): State<AppState>,
    request: Request,
) -> Result<Response, S3Error> {
    let operation = request
        .extensions()
        .get::<S3Operation>()
        .cloned()
        .ok_or_else(|| {
            S3Error::new(
                S3ErrorCode::InternalError,
                "No S3 operation found in request",
                "",
            )
        })?;

    // Extract custom metadata headers and body for PutObject
    let custom_metadata = extract_custom_metadata(&request);
    let content_type = request
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    match operation {
        S3Operation::ListBuckets => handlers::bucket::list_buckets(&state).await,
        S3Operation::CreateBucket { bucket } => {
            handlers::bucket::create_bucket(&state, &bucket).await
        }
        S3Operation::DeleteBucket { bucket } => {
            handlers::bucket::delete_bucket(&state, &bucket).await
        }
        S3Operation::HeadBucket { bucket } => {
            handlers::bucket::head_bucket(&state, &bucket).await
        }
        S3Operation::ListObjectsV2 { bucket, params } => {
            handlers::object::list_objects_v2(&state, &bucket, &params).await
        }
        S3Operation::PutObject { bucket, key } => {
            let content_length = request
                .headers()
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok());
            let body = request.into_body();
            handlers::object::put_object(
                &state,
                &bucket,
                &key,
                body,
                content_type,
                custom_metadata,
                content_length,
            )
            .await
        }
        S3Operation::GetObject { bucket, key } => {
            handlers::object::get_object(&state, &bucket, &key).await
        }
        S3Operation::DeleteObject { bucket, key } => {
            handlers::object::delete_object(&state, &bucket, &key).await
        }
        S3Operation::HeadObject { bucket, key } => {
            handlers::object::head_object(&state, &bucket, &key).await
        }
    }
}

fn extract_custom_metadata(request: &Request) -> std::collections::HashMap<String, String> {
    let mut meta = std::collections::HashMap::new();
    for (name, value) in request.headers() {
        let name_str = name.as_str().to_lowercase();
        if let Some(key) = name_str.strip_prefix("x-amz-meta-") {
            if let Ok(val) = value.to_str() {
                meta.insert(key.to_string(), val.to_string());
            }
        }
    }
    meta
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request as HttpRequest;

    #[test]
    fn extract_custom_metadata_empty() {
        let req = HttpRequest::builder()
            .header("content-type", "text/plain")
            .header("authorization", "none")
            .body(())
            .unwrap();
        let req = Request::from_parts(req.into_parts().0, axum::body::Body::empty());
        let meta = extract_custom_metadata(&req);
        assert!(meta.is_empty());
    }

    #[test]
    fn extract_custom_metadata_with_values() {
        let req = HttpRequest::builder()
            .header("x-amz-meta-author", "alice")
            .header("x-amz-meta-version", "1.0")
            .header("content-type", "text/plain")
            .body(())
            .unwrap();
        let req = Request::from_parts(req.into_parts().0, axum::body::Body::empty());
        let meta = extract_custom_metadata(&req);
        assert_eq!(meta.get("author").unwrap(), "alice");
        assert_eq!(meta.get("version").unwrap(), "1.0");
        assert_eq!(meta.len(), 2);
    }

    #[test]
    fn extract_custom_metadata_ignores_non_meta_headers() {
        let req = HttpRequest::builder()
            .header("x-amz-request-id", "12345")
            .header("x-amz-meta-tag", "important")
            .body(())
            .unwrap();
        let req = Request::from_parts(req.into_parts().0, axum::body::Body::empty());
        let meta = extract_custom_metadata(&req);
        assert_eq!(meta.len(), 1);
        assert_eq!(meta.get("tag").unwrap(), "important");
    }
}

