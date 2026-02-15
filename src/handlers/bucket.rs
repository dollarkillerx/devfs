use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::error::S3Error;
use crate::types::AppState;
use crate::xml::*;

pub async fn list_buckets(state: &AppState) -> Result<Response, S3Error> {
    let buckets = state.storage.list_buckets().await?;

    let result = ListAllMyBucketsResult {
        xmlns: S3_XMLNS,
        owner: Owner {
            id: "devfs".to_string(),
            display_name: "devfs".to_string(),
        },
        buckets: Buckets {
            bucket: buckets
                .into_iter()
                .map(|(name, date)| BucketInfo {
                    name,
                    creation_date: date,
                })
                .collect(),
        },
    };

    let xml = to_s3_xml(&result).map_err(|e| S3Error::internal(e.to_string()))?;
    Ok(xml_response(StatusCode::OK, xml))
}

pub async fn create_bucket(state: &AppState, bucket: &str) -> Result<Response, S3Error> {
    state.storage.create_bucket(bucket).await?;
    Ok((
        StatusCode::OK,
        s3_headers(),
        [("location", format!("/{}", bucket))],
    )
        .into_response())
}

pub async fn delete_bucket(state: &AppState, bucket: &str) -> Result<Response, S3Error> {
    state.storage.delete_bucket(bucket).await?;
    Ok((StatusCode::NO_CONTENT, s3_headers()).into_response())
}

pub async fn head_bucket(state: &AppState, bucket: &str) -> Result<Response, S3Error> {
    state.storage.head_bucket(bucket).await?;
    Ok((StatusCode::OK, s3_headers()).into_response())
}

fn xml_response(status: StatusCode, body: String) -> Response {
    (
        status,
        [
            ("content-type", "application/xml".to_string()),
            ("server", "devfs".to_string()),
            (
                "x-amz-request-id",
                uuid::Uuid::new_v4().to_string(),
            ),
        ],
        body,
    )
        .into_response()
}

fn s3_headers() -> [(String, String); 2] {
    [
        ("server".to_string(), "devfs".to_string()),
        (
            "x-amz-request-id".to_string(),
            uuid::Uuid::new_v4().to_string(),
        ),
    ]
}
