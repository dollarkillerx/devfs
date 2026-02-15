use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

use crate::xml::to_s3_xml;

#[derive(Debug, Clone)]
pub enum S3ErrorCode {
    AccessDenied,
    BucketAlreadyExists,
    BucketNotEmpty,
    InternalError,
    InvalidBucketName,
    NoSuchBucket,
    NoSuchKey,
    InvalidArgument,
}

impl S3ErrorCode {
    pub fn status_code(&self) -> StatusCode {
        match self {
            S3ErrorCode::AccessDenied => StatusCode::FORBIDDEN,
            S3ErrorCode::BucketAlreadyExists => StatusCode::CONFLICT,
            S3ErrorCode::BucketNotEmpty => StatusCode::CONFLICT,
            S3ErrorCode::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
            S3ErrorCode::InvalidBucketName => StatusCode::BAD_REQUEST,
            S3ErrorCode::NoSuchBucket => StatusCode::NOT_FOUND,
            S3ErrorCode::NoSuchKey => StatusCode::NOT_FOUND,
            S3ErrorCode::InvalidArgument => StatusCode::BAD_REQUEST,
        }
    }

    pub fn code_str(&self) -> &'static str {
        match self {
            S3ErrorCode::AccessDenied => "AccessDenied",
            S3ErrorCode::BucketAlreadyExists => "BucketAlreadyExists",
            S3ErrorCode::BucketNotEmpty => "BucketNotEmpty",
            S3ErrorCode::InternalError => "InternalError",
            S3ErrorCode::InvalidBucketName => "InvalidBucketName",
            S3ErrorCode::NoSuchBucket => "NoSuchBucket",
            S3ErrorCode::NoSuchKey => "NoSuchKey",
            S3ErrorCode::InvalidArgument => "InvalidArgument",
        }
    }
}

#[derive(Debug)]
pub struct S3Error {
    pub code: S3ErrorCode,
    pub message: String,
    pub resource: String,
}

impl S3Error {
    pub fn new(code: S3ErrorCode, message: impl Into<String>, resource: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            resource: resource.into(),
        }
    }

    pub fn no_such_bucket(bucket: &str) -> Self {
        Self::new(
            S3ErrorCode::NoSuchBucket,
            format!("The specified bucket does not exist: {}", bucket),
            bucket,
        )
    }

    pub fn no_such_key(key: &str) -> Self {
        Self::new(
            S3ErrorCode::NoSuchKey,
            format!("The specified key does not exist: {}", key),
            key,
        )
    }

    pub fn bucket_not_empty(bucket: &str) -> Self {
        Self::new(
            S3ErrorCode::BucketNotEmpty,
            "The bucket you tried to delete is not empty",
            bucket,
        )
    }

    pub fn bucket_already_exists(bucket: &str) -> Self {
        Self::new(
            S3ErrorCode::BucketAlreadyExists,
            "The requested bucket name already exists",
            bucket,
        )
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(S3ErrorCode::InternalError, msg, "")
    }

    pub fn access_denied() -> Self {
        Self::new(S3ErrorCode::AccessDenied, "Access Denied", "")
    }
}

#[derive(Serialize)]
#[serde(rename = "Error")]
struct ErrorXml {
    #[serde(rename = "Code")]
    code: String,
    #[serde(rename = "Message")]
    message: String,
    #[serde(rename = "Resource")]
    resource: String,
    #[serde(rename = "RequestId")]
    request_id: String,
}

impl IntoResponse for S3Error {
    fn into_response(self) -> Response {
        let error_xml = ErrorXml {
            code: self.code.code_str().to_string(),
            message: self.message,
            resource: self.resource,
            request_id: uuid::Uuid::new_v4().to_string(),
        };

        let body = to_s3_xml(&error_xml).unwrap_or_else(|_| {
            "<Error><Code>InternalError</Code><Message>Failed to serialize error</Message></Error>"
                .to_string()
        });

        (
            self.code.status_code(),
            [
                ("content-type", "application/xml"),
                ("server", "devfs"),
                ("x-amz-request-id", &uuid::Uuid::new_v4().to_string()),
            ],
            body,
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_code_mapping() {
        assert_eq!(S3ErrorCode::AccessDenied.status_code(), StatusCode::FORBIDDEN);
        assert_eq!(S3ErrorCode::BucketAlreadyExists.status_code(), StatusCode::CONFLICT);
        assert_eq!(S3ErrorCode::BucketNotEmpty.status_code(), StatusCode::CONFLICT);
        assert_eq!(S3ErrorCode::InternalError.status_code(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(S3ErrorCode::InvalidBucketName.status_code(), StatusCode::BAD_REQUEST);
        assert_eq!(S3ErrorCode::NoSuchBucket.status_code(), StatusCode::NOT_FOUND);
        assert_eq!(S3ErrorCode::NoSuchKey.status_code(), StatusCode::NOT_FOUND);
        assert_eq!(S3ErrorCode::InvalidArgument.status_code(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn code_str_mapping() {
        assert_eq!(S3ErrorCode::AccessDenied.code_str(), "AccessDenied");
        assert_eq!(S3ErrorCode::BucketAlreadyExists.code_str(), "BucketAlreadyExists");
        assert_eq!(S3ErrorCode::BucketNotEmpty.code_str(), "BucketNotEmpty");
        assert_eq!(S3ErrorCode::InternalError.code_str(), "InternalError");
        assert_eq!(S3ErrorCode::InvalidBucketName.code_str(), "InvalidBucketName");
        assert_eq!(S3ErrorCode::NoSuchBucket.code_str(), "NoSuchBucket");
        assert_eq!(S3ErrorCode::NoSuchKey.code_str(), "NoSuchKey");
        assert_eq!(S3ErrorCode::InvalidArgument.code_str(), "InvalidArgument");
    }

    #[test]
    fn factory_no_such_bucket() {
        let err = S3Error::no_such_bucket("my-bucket");
        assert!(matches!(err.code, S3ErrorCode::NoSuchBucket));
        assert!(err.message.contains("my-bucket"));
        assert_eq!(err.resource, "my-bucket");
    }

    #[test]
    fn factory_no_such_key() {
        let err = S3Error::no_such_key("my-key");
        assert!(matches!(err.code, S3ErrorCode::NoSuchKey));
        assert!(err.message.contains("my-key"));
        assert_eq!(err.resource, "my-key");
    }

    #[test]
    fn factory_bucket_not_empty() {
        let err = S3Error::bucket_not_empty("my-bucket");
        assert!(matches!(err.code, S3ErrorCode::BucketNotEmpty));
        assert_eq!(err.resource, "my-bucket");
    }

    #[test]
    fn factory_bucket_already_exists() {
        let err = S3Error::bucket_already_exists("my-bucket");
        assert!(matches!(err.code, S3ErrorCode::BucketAlreadyExists));
        assert_eq!(err.resource, "my-bucket");
    }

    #[test]
    fn factory_internal() {
        let err = S3Error::internal("something broke");
        assert!(matches!(err.code, S3ErrorCode::InternalError));
        assert_eq!(err.message, "something broke");
        assert_eq!(err.resource, "");
    }

    #[test]
    fn factory_access_denied() {
        let err = S3Error::access_denied();
        assert!(matches!(err.code, S3ErrorCode::AccessDenied));
        assert_eq!(err.message, "Access Denied");
        assert_eq!(err.resource, "");
    }

    #[test]
    fn into_response_status_and_content_type() {
        let err = S3Error::no_such_bucket("test");
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/xml"
        );
    }
}
