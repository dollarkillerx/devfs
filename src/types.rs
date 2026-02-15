use serde::Serialize;
use std::path::PathBuf;

use crate::storage::FsStorage;

/// S3 operation parsed from HTTP request
#[derive(Debug, Clone)]
pub enum S3Operation {
    ListBuckets,
    CreateBucket { bucket: String },
    DeleteBucket { bucket: String },
    HeadBucket { bucket: String },
    ListObjectsV2 { bucket: String, params: ListObjectsV2Params },
    PutObject { bucket: String, key: String },
    GetObject { bucket: String, key: String },
    DeleteObject { bucket: String, key: String },
    HeadObject { bucket: String, key: String },
}

#[derive(Debug, Clone, Default)]
pub struct ListObjectsV2Params {
    pub prefix: Option<String>,
    pub delimiter: Option<String>,
    pub max_keys: u32,
    pub continuation_token: Option<String>,
    pub start_after: Option<String>,
}

#[derive(Clone)]
pub struct AppState {
    pub storage: FsStorage,
    pub auth_config: Option<AuthConfig>,
}

#[derive(Clone, Debug)]
pub struct AuthConfig {
    pub access_key: String,
    pub secret_key: String,
}

/// Object metadata stored as JSON
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct ObjectMetadata {
    pub content_type: String,
    pub content_length: u64,
    pub etag: String,
    pub last_modified: String,
    #[serde(default)]
    pub custom_metadata: std::collections::HashMap<String, String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            storage: FsStorage::new(&PathBuf::from("./data")),
            auth_config: None,
        }
    }
}
