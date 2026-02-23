use std::collections::HashMap;

use axum::body::Body;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use tokio::io::BufReader;
use tokio_util::io::ReaderStream;

use crate::error::S3Error;
use crate::types::{AppState, ListObjectsV2Params};
use crate::xml::*;

pub async fn put_object(
    state: &AppState,
    bucket: &str,
    key: &str,
    body: Body,
    content_type: Option<String>,
    custom_metadata: HashMap<String, String>,
    content_length: Option<u64>,
) -> Result<Response, S3Error> {
    let result = state
        .storage
        .put_object_stream(bucket, key, body, content_type, custom_metadata, content_length)
        .await?;

    Response::builder()
        .status(StatusCode::OK)
        .header("etag", &result.etag)
        .header("server", "devfs")
        .header("x-amz-request-id", uuid::Uuid::new_v4().to_string())
        .header("x-amz-checksum-crc32", &result.checksum_crc32)
        .body(Body::empty())
        .map_err(|e| S3Error::internal(e.to_string()))
}

pub async fn get_object(
    state: &AppState,
    bucket: &str,
    key: &str,
) -> Result<Response, S3Error> {
    let result = state.storage.get_object_stream(bucket, key).await?;
    let meta = &result.metadata;

    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", &meta.content_type)
        .header("content-length", meta.content_length.to_string())
        .header("etag", &meta.etag)
        .header("last-modified", http_date(&meta.last_modified))
        .header("server", "devfs")
        .header("x-amz-request-id", uuid::Uuid::new_v4().to_string());

    if let Some(ref crc32) = meta.checksum_crc32 {
        builder = builder.header("x-amz-checksum-crc32", crc32);
    }

    for (k, v) in &meta.custom_metadata {
        builder = builder.header(format!("x-amz-meta-{}", k), v);
    }

    let reader = BufReader::with_capacity(64 * 1024, result.file);
    let stream = ReaderStream::with_capacity(reader, 64 * 1024);
    let body = Body::from_stream(stream);

    builder
        .body(body)
        .map_err(|e| S3Error::internal(e.to_string()))
}

pub async fn delete_object(
    state: &AppState,
    bucket: &str,
    key: &str,
) -> Result<Response, S3Error> {
    state.storage.delete_object(bucket, key).await?;
    Ok((
        StatusCode::NO_CONTENT,
        [
            ("server", "devfs".to_string()),
            ("x-amz-request-id", uuid::Uuid::new_v4().to_string()),
        ],
    )
        .into_response())
}

pub async fn head_object(
    state: &AppState,
    bucket: &str,
    key: &str,
) -> Result<Response, S3Error> {
    let meta = state.storage.head_object(bucket, key).await?;

    let mut builder = Response::builder()
        .status(StatusCode::OK)
        .header("content-type", &meta.content_type)
        .header("content-length", meta.content_length.to_string())
        .header("etag", &meta.etag)
        .header("last-modified", http_date(&meta.last_modified))
        .header("server", "devfs")
        .header("x-amz-request-id", uuid::Uuid::new_v4().to_string());

    if let Some(ref crc32) = meta.checksum_crc32 {
        builder = builder.header("x-amz-checksum-crc32", crc32);
    }

    for (k, v) in &meta.custom_metadata {
        builder = builder.header(format!("x-amz-meta-{}", k), v);
    }

    builder
        .body(Body::empty())
        .map_err(|e| S3Error::internal(e.to_string()))
}

pub async fn list_objects_v2(
    state: &AppState,
    bucket: &str,
    params: &ListObjectsV2Params,
) -> Result<Response, S3Error> {
    let result = state.storage.list_objects_v2(bucket, params).await?;

    let list_result = ListBucketResult {
        xmlns: S3_XMLNS,
        name: bucket.to_string(),
        prefix: params.prefix.clone().unwrap_or_default(),
        delimiter: params.delimiter.clone(),
        max_keys: params.max_keys,
        key_count: result.objects.len() as u32 + result.common_prefixes.len() as u32,
        is_truncated: result.is_truncated,
        contents: result
            .objects
            .into_iter()
            .map(|o| ObjectInfo {
                key: o.key,
                last_modified: o.last_modified,
                etag: o.etag,
                size: o.size,
                storage_class: "STANDARD".to_string(),
            })
            .collect(),
        common_prefixes: result
            .common_prefixes
            .into_iter()
            .map(|p| CommonPrefix { prefix: p })
            .collect(),
        continuation_token: params.continuation_token.clone(),
        next_continuation_token: result.next_continuation_token,
        start_after: params.start_after.clone(),
    };

    let xml = to_s3_xml(&list_result).map_err(|e| S3Error::internal(e.to_string()))?;

    Ok((
        StatusCode::OK,
        [
            ("content-type", "application/xml".to_string()),
            ("server", "devfs".to_string()),
            ("x-amz-request-id", uuid::Uuid::new_v4().to_string()),
        ],
        xml,
    )
        .into_response())
}

/// Convert ISO 8601 timestamp to HTTP-date format (RFC 7231)
fn http_date(iso: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(iso)
        .map(|dt| dt.format("%a, %d %b %Y %H:%M:%S GMT").to_string())
        .unwrap_or_else(|_| iso.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_date_valid_rfc3339() {
        let result = http_date("2024-01-15T10:30:00+00:00");
        assert_eq!(result, "Mon, 15 Jan 2024 10:30:00 GMT");
    }

    #[test]
    fn http_date_invalid_input_returned_as_is() {
        let result = http_date("not-a-date");
        assert_eq!(result, "not-a-date");
    }
}
