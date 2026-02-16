use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::keystore::KeyStore;
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

impl S3Operation {
    pub fn bucket_name(&self) -> Option<&str> {
        match self {
            S3Operation::ListBuckets => None,
            S3Operation::CreateBucket { bucket }
            | S3Operation::DeleteBucket { bucket }
            | S3Operation::HeadBucket { bucket }
            | S3Operation::ListObjectsV2 { bucket, .. }
            | S3Operation::PutObject { bucket, .. }
            | S3Operation::GetObject { bucket, .. }
            | S3Operation::DeleteObject { bucket, .. }
            | S3Operation::HeadObject { bucket, .. } => Some(bucket),
        }
    }

    pub fn is_read_only(&self) -> bool {
        matches!(
            self,
            S3Operation::ListBuckets
                | S3Operation::HeadBucket { .. }
                | S3Operation::ListObjectsV2 { .. }
                | S3Operation::GetObject { .. }
                | S3Operation::HeadObject { .. }
        )
    }
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
    pub web_auth_config: Option<WebAuthConfig>,
    pub key_store: Arc<RwLock<KeyStore>>,
    pub jwt_secret: String,
}

#[derive(Clone, Debug)]
pub struct AuthConfig {
    pub access_key: String,
    pub secret_key: String,
}

#[derive(Clone, Debug)]
pub struct WebAuthConfig {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyEntry {
    pub id: String,
    pub access_key: String,
    pub secret_key: String,
    pub description: String,
    pub buckets: HashMap<String, BucketPermission>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum BucketPermission {
    None,
    Read,
    ReadWrite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BucketPolicy {
    pub public_read: bool,
    pub public_write: bool,
}

impl Default for BucketPolicy {
    fn default() -> Self {
        Self {
            public_read: false,
            public_write: false,
        }
    }
}

/// Object metadata stored as JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectMetadata {
    pub content_type: String,
    pub content_length: u64,
    pub etag: String,
    pub last_modified: String,
    #[serde(default)]
    pub custom_metadata: HashMap<String, String>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            storage: FsStorage::new(&PathBuf::from("./data")),
            auth_config: None,
            web_auth_config: None,
            key_store: Arc::new(RwLock::new(KeyStore::empty())),
            jwt_secret: String::new(),
        }
    }
}
