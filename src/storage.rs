use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use chrono::Utc;
use md5::{Digest, Md5};
use tokio::fs;
use tokio::io::AsyncWriteExt;

use crate::error::S3Error;
use crate::types::{ListObjectsV2Params, ObjectMetadata};

#[derive(Clone)]
pub struct FsStorage {
    root: PathBuf,
}

#[derive(Debug)]
pub struct GetObjectResult {
    pub metadata: ObjectMetadata,
    pub body: Vec<u8>,
}

pub struct ListObjectsV2Result {
    pub objects: Vec<ObjectEntry>,
    pub common_prefixes: Vec<String>,
    pub is_truncated: bool,
    pub next_continuation_token: Option<String>,
}

pub struct ObjectEntry {
    pub key: String,
    pub size: u64,
    pub etag: String,
    pub last_modified: String,
}

impl FsStorage {
    pub fn new(root: &Path) -> Self {
        // Ensure data directory exists at startup
        std::fs::create_dir_all(root).expect("Failed to create data directory");
        Self {
            root: root.to_path_buf(),
        }
    }

    fn bucket_path(&self, bucket: &str) -> PathBuf {
        self.root.join(bucket)
    }

    fn object_path(&self, bucket: &str, key: &str) -> PathBuf {
        self.root.join(bucket).join(key)
    }

    fn meta_path(&self, bucket: &str, key: &str) -> PathBuf {
        self.root.join(bucket).join(".meta").join(format!("{}.json", key))
    }

    pub async fn list_buckets(&self) -> Result<Vec<(String, String)>, S3Error> {
        let mut buckets = Vec::new();

        let mut entries = fs::read_dir(&self.root)
            .await
            .map_err(|e| S3Error::internal(format!("Failed to read data dir: {}", e)))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| S3Error::internal(e.to_string()))?
        {
            let ft = entry
                .file_type()
                .await
                .map_err(|e| S3Error::internal(e.to_string()))?;
            if ft.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with('.') {
                    let metadata = entry
                        .metadata()
                        .await
                        .map_err(|e| S3Error::internal(e.to_string()))?;
                    let created = metadata
                        .created()
                        .unwrap_or_else(|_| std::time::SystemTime::now());
                    let dt: chrono::DateTime<Utc> = created.into();
                    buckets.push((name, dt.to_rfc3339()));
                }
            }
        }

        buckets.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(buckets)
    }

    pub async fn create_bucket(&self, bucket: &str) -> Result<(), S3Error> {
        let path = self.bucket_path(bucket);
        if path.exists() {
            return Err(S3Error::bucket_already_exists(bucket));
        }
        fs::create_dir_all(&path)
            .await
            .map_err(|e| S3Error::internal(format!("Failed to create bucket: {}", e)))?;
        fs::create_dir_all(path.join(".meta"))
            .await
            .map_err(|e| S3Error::internal(format!("Failed to create meta dir: {}", e)))?;
        Ok(())
    }

    pub async fn delete_bucket(&self, bucket: &str) -> Result<(), S3Error> {
        let path = self.bucket_path(bucket);
        if !path.exists() {
            return Err(S3Error::no_such_bucket(bucket));
        }

        // Check if bucket has any objects (non-.meta entries)
        let mut entries = fs::read_dir(&path)
            .await
            .map_err(|e| S3Error::internal(e.to_string()))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| S3Error::internal(e.to_string()))?
        {
            let name = entry.file_name().to_string_lossy().to_string();
            if name != ".meta" {
                return Err(S3Error::bucket_not_empty(bucket));
            }
        }

        fs::remove_dir_all(&path)
            .await
            .map_err(|e| S3Error::internal(format!("Failed to delete bucket: {}", e)))?;
        Ok(())
    }

    pub async fn head_bucket(&self, bucket: &str) -> Result<(), S3Error> {
        let path = self.bucket_path(bucket);
        if !path.exists() {
            return Err(S3Error::no_such_bucket(bucket));
        }
        Ok(())
    }

    pub async fn put_object(
        &self,
        bucket: &str,
        key: &str,
        body: bytes::Bytes,
        content_type: Option<String>,
        custom_metadata: HashMap<String, String>,
    ) -> Result<String, S3Error> {
        let bucket_path = self.bucket_path(bucket);
        if !bucket_path.exists() {
            return Err(S3Error::no_such_bucket(bucket));
        }

        // Compute MD5 ETag
        let mut hasher = Md5::new();
        hasher.update(&body);
        let etag = format!("\"{}\"", hex::encode(hasher.finalize()));

        // Write object file
        let obj_path = self.object_path(bucket, key);
        if let Some(parent) = obj_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| S3Error::internal(format!("Failed to create dirs: {}", e)))?;
        }

        let mut file = fs::File::create(&obj_path)
            .await
            .map_err(|e| S3Error::internal(format!("Failed to create file: {}", e)))?;
        file.write_all(&body)
            .await
            .map_err(|e| S3Error::internal(format!("Failed to write file: {}", e)))?;

        // Determine content type
        let ct = content_type.unwrap_or_else(|| {
            mime_guess::from_path(key)
                .first_or_octet_stream()
                .to_string()
        });

        // Write metadata
        let now = Utc::now().to_rfc3339();
        let metadata = ObjectMetadata {
            content_type: ct,
            content_length: body.len() as u64,
            etag: etag.clone(),
            last_modified: now,
            custom_metadata,
        };

        let meta_path = self.meta_path(bucket, key);
        if let Some(parent) = meta_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| S3Error::internal(format!("Failed to create meta dirs: {}", e)))?;
        }

        let meta_json = serde_json::to_string_pretty(&metadata)
            .map_err(|e| S3Error::internal(format!("Failed to serialize metadata: {}", e)))?;
        fs::write(&meta_path, meta_json)
            .await
            .map_err(|e| S3Error::internal(format!("Failed to write metadata: {}", e)))?;

        Ok(etag)
    }

    pub async fn get_object(&self, bucket: &str, key: &str) -> Result<GetObjectResult, S3Error> {
        let bucket_path = self.bucket_path(bucket);
        if !bucket_path.exists() {
            return Err(S3Error::no_such_bucket(bucket));
        }

        let obj_path = self.object_path(bucket, key);
        if !obj_path.exists() || obj_path.is_dir() {
            return Err(S3Error::no_such_key(key));
        }

        let metadata = self.read_metadata(bucket, key).await?;
        let body = fs::read(&obj_path)
            .await
            .map_err(|e| S3Error::internal(format!("Failed to read object: {}", e)))?;

        Ok(GetObjectResult { metadata, body })
    }

    pub async fn delete_object(&self, bucket: &str, key: &str) -> Result<(), S3Error> {
        let bucket_path = self.bucket_path(bucket);
        if !bucket_path.exists() {
            return Err(S3Error::no_such_bucket(bucket));
        }

        // Idempotent: ignore if not exists
        let obj_path = self.object_path(bucket, key);
        if obj_path.exists() {
            let _ = fs::remove_file(&obj_path).await;
        }

        let meta_path = self.meta_path(bucket, key);
        if meta_path.exists() {
            let _ = fs::remove_file(&meta_path).await;
        }

        // Clean up empty parent directories (but not the bucket itself)
        self.cleanup_empty_parents(bucket, key).await;

        Ok(())
    }

    pub async fn head_object(&self, bucket: &str, key: &str) -> Result<ObjectMetadata, S3Error> {
        let bucket_path = self.bucket_path(bucket);
        if !bucket_path.exists() {
            return Err(S3Error::no_such_bucket(bucket));
        }

        let obj_path = self.object_path(bucket, key);
        if !obj_path.exists() || obj_path.is_dir() {
            return Err(S3Error::no_such_key(key));
        }

        self.read_metadata(bucket, key).await
    }

    pub async fn list_objects_v2(
        &self,
        bucket: &str,
        params: &ListObjectsV2Params,
    ) -> Result<ListObjectsV2Result, S3Error> {
        let bucket_path = self.bucket_path(bucket);
        if !bucket_path.exists() {
            return Err(S3Error::no_such_bucket(bucket));
        }

        let prefix = params.prefix.as_deref().unwrap_or("");
        let max_keys = params.max_keys;

        // Collect all keys recursively
        let mut all_keys = Vec::new();
        self.collect_keys(&bucket_path, &bucket_path, &mut all_keys)
            .await?;
        all_keys.sort();

        // Apply prefix filter
        let filtered: Vec<String> = all_keys
            .into_iter()
            .filter(|k| k.starts_with(prefix))
            .collect();

        // Apply start_after / continuation_token
        let start_after = params
            .continuation_token
            .as_deref()
            .or(params.start_after.as_deref());

        let filtered: Vec<String> = if let Some(start) = start_after {
            filtered.into_iter().filter(|k| k.as_str() > start).collect()
        } else {
            filtered
        };

        // Handle delimiter grouping
        if let Some(delimiter) = &params.delimiter {
            let mut objects = Vec::new();
            let mut common_prefixes = BTreeSet::new();

            for key in &filtered {
                let after_prefix = &key[prefix.len()..];
                if let Some(pos) = after_prefix.find(delimiter.as_str()) {
                    let cp = format!("{}{}", prefix, &after_prefix[..=pos + delimiter.len() - 1]);
                    common_prefixes.insert(cp);
                } else {
                    objects.push(key.clone());
                }
            }

            let total_items = objects.len() + common_prefixes.len();
            let is_truncated = total_items > max_keys as usize;

            // Merge and truncate
            let mut result_objects = Vec::new();
            let mut result_prefixes: Vec<String> = Vec::new();
            let mut count = 0u32;
            let mut last_key = None;

            let cp_vec: Vec<String> = common_prefixes.into_iter().collect();
            let mut obj_idx = 0;
            let mut cp_idx = 0;

            while count < max_keys && (obj_idx < objects.len() || cp_idx < cp_vec.len()) {
                let use_obj = if obj_idx < objects.len() && cp_idx < cp_vec.len() {
                    objects[obj_idx] < cp_vec[cp_idx]
                } else {
                    obj_idx < objects.len()
                };

                if use_obj {
                    let key = &objects[obj_idx];
                    let meta = self.read_metadata(bucket, key).await?;
                    last_key = Some(key.clone());
                    result_objects.push(ObjectEntry {
                        key: key.clone(),
                        size: meta.content_length,
                        etag: meta.etag,
                        last_modified: meta.last_modified,
                    });
                    obj_idx += 1;
                } else {
                    last_key = Some(cp_vec[cp_idx].clone());
                    result_prefixes.push(cp_vec[cp_idx].clone());
                    cp_idx += 1;
                }
                count += 1;
            }

            Ok(ListObjectsV2Result {
                objects: result_objects,
                common_prefixes: result_prefixes,
                is_truncated,
                next_continuation_token: if is_truncated { last_key } else { None },
            })
        } else {
            // No delimiter — flat listing
            let is_truncated = filtered.len() > max_keys as usize;
            let keys: Vec<String> = filtered.into_iter().take(max_keys as usize).collect();
            let last_key = keys.last().cloned();

            let mut objects = Vec::new();
            for key in &keys {
                let meta = self.read_metadata(bucket, key).await?;
                objects.push(ObjectEntry {
                    key: key.clone(),
                    size: meta.content_length,
                    etag: meta.etag,
                    last_modified: meta.last_modified,
                });
            }

            Ok(ListObjectsV2Result {
                objects,
                common_prefixes: Vec::new(),
                is_truncated,
                next_continuation_token: if is_truncated { last_key } else { None },
            })
        }
    }

    async fn read_metadata(&self, bucket: &str, key: &str) -> Result<ObjectMetadata, S3Error> {
        let meta_path = self.meta_path(bucket, key);
        let data = fs::read_to_string(&meta_path)
            .await
            .map_err(|_| S3Error::no_such_key(key))?;
        serde_json::from_str(&data)
            .map_err(|e| S3Error::internal(format!("Corrupt metadata for {}: {}", key, e)))
    }

    async fn collect_keys(
        &self,
        bucket_path: &Path,
        dir: &Path,
        keys: &mut Vec<String>,
    ) -> Result<(), S3Error> {
        let mut entries = match fs::read_dir(dir).await {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| S3Error::internal(e.to_string()))?
        {
            let name = entry.file_name().to_string_lossy().to_string();
            if name == ".meta" {
                continue;
            }

            let path = entry.path();
            let ft = entry
                .file_type()
                .await
                .map_err(|e| S3Error::internal(e.to_string()))?;

            if ft.is_dir() {
                Box::pin(self.collect_keys(bucket_path, &path, keys)).await?;
            } else {
                let key = path
                    .strip_prefix(bucket_path)
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
                keys.push(key);
            }
        }

        Ok(())
    }

    async fn cleanup_empty_parents(&self, bucket: &str, key: &str) {
        let bucket_path = self.bucket_path(bucket);
        let obj_path = self.object_path(bucket, key);

        let mut current = obj_path.parent().map(|p| p.to_path_buf());
        while let Some(dir) = current {
            if dir == bucket_path {
                break;
            }
            // Try to remove — will fail if not empty, which is fine
            if fs::remove_dir(&dir).await.is_err() {
                break;
            }
            current = dir.parent().map(|p| p.to_path_buf());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::S3ErrorCode;
    use bytes::Bytes;

    fn make_storage(dir: &std::path::Path) -> FsStorage {
        FsStorage::new(dir)
    }

    // --- Bucket operations ---

    #[tokio::test]
    async fn create_bucket_creates_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = make_storage(tmp.path());
        storage.create_bucket("test-bucket").await.unwrap();

        assert!(tmp.path().join("test-bucket").is_dir());
        assert!(tmp.path().join("test-bucket").join(".meta").is_dir());
    }

    #[tokio::test]
    async fn create_bucket_duplicate_error() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = make_storage(tmp.path());
        storage.create_bucket("dup").await.unwrap();

        let err = storage.create_bucket("dup").await.unwrap_err();
        assert!(matches!(err.code, S3ErrorCode::BucketAlreadyExists));
    }

    #[tokio::test]
    async fn head_bucket_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = make_storage(tmp.path());
        storage.create_bucket("exists").await.unwrap();
        storage.head_bucket("exists").await.unwrap();
    }

    #[tokio::test]
    async fn head_bucket_not_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = make_storage(tmp.path());
        let err = storage.head_bucket("nope").await.unwrap_err();
        assert!(matches!(err.code, S3ErrorCode::NoSuchBucket));
    }

    #[tokio::test]
    async fn delete_bucket_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = make_storage(tmp.path());
        storage.create_bucket("del").await.unwrap();
        storage.delete_bucket("del").await.unwrap();
        assert!(!tmp.path().join("del").exists());
    }

    #[tokio::test]
    async fn delete_bucket_not_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = make_storage(tmp.path());
        storage.create_bucket("notempty").await.unwrap();
        storage
            .put_object("notempty", "file.txt", Bytes::from("data"), None, HashMap::new())
            .await
            .unwrap();
        let err = storage.delete_bucket("notempty").await.unwrap_err();
        assert!(matches!(err.code, S3ErrorCode::BucketNotEmpty));
    }

    #[tokio::test]
    async fn delete_bucket_not_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = make_storage(tmp.path());
        let err = storage.delete_bucket("ghost").await.unwrap_err();
        assert!(matches!(err.code, S3ErrorCode::NoSuchBucket));
    }

    #[tokio::test]
    async fn list_buckets_sorted() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = make_storage(tmp.path());
        storage.create_bucket("charlie").await.unwrap();
        storage.create_bucket("alpha").await.unwrap();
        storage.create_bucket("bravo").await.unwrap();

        let buckets = storage.list_buckets().await.unwrap();
        let names: Vec<&str> = buckets.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["alpha", "bravo", "charlie"]);
    }

    // --- Object operations ---

    #[tokio::test]
    async fn put_get_object_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = make_storage(tmp.path());
        storage.create_bucket("b").await.unwrap();

        let mut meta = HashMap::new();
        meta.insert("author".to_string(), "alice".to_string());

        let etag = storage
            .put_object(
                "b",
                "hello.txt",
                Bytes::from("hello world"),
                Some("text/plain".to_string()),
                meta.clone(),
            )
            .await
            .unwrap();

        let result = storage.get_object("b", "hello.txt").await.unwrap();
        assert_eq!(result.body, b"hello world");
        assert_eq!(result.metadata.content_type, "text/plain");
        assert_eq!(result.metadata.etag, etag);
        assert_eq!(
            result.metadata.custom_metadata.get("author").unwrap(),
            "alice"
        );
    }

    #[tokio::test]
    async fn put_object_no_such_bucket() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = make_storage(tmp.path());
        let err = storage
            .put_object("nope", "k", Bytes::from("x"), None, HashMap::new())
            .await
            .unwrap_err();
        assert!(matches!(err.code, S3ErrorCode::NoSuchBucket));
    }

    #[tokio::test]
    async fn get_object_no_such_key() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = make_storage(tmp.path());
        storage.create_bucket("b").await.unwrap();
        let err = storage.get_object("b", "missing").await.unwrap_err();
        assert!(matches!(err.code, S3ErrorCode::NoSuchKey));
    }

    #[tokio::test]
    async fn delete_object_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = make_storage(tmp.path());
        storage.create_bucket("b").await.unwrap();
        // Delete non-existent key should not error
        storage.delete_object("b", "nope").await.unwrap();
    }

    #[tokio::test]
    async fn head_object_returns_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = make_storage(tmp.path());
        storage.create_bucket("b").await.unwrap();
        storage
            .put_object(
                "b",
                "test.json",
                Bytes::from("{\"a\":1}"),
                Some("application/json".to_string()),
                HashMap::new(),
            )
            .await
            .unwrap();

        let meta = storage.head_object("b", "test.json").await.unwrap();
        assert_eq!(meta.content_type, "application/json");
        assert_eq!(meta.content_length, 7);
    }

    #[tokio::test]
    async fn put_object_nested_key_creates_parents() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = make_storage(tmp.path());
        storage.create_bucket("b").await.unwrap();
        storage
            .put_object(
                "b",
                "dir/subdir/file.txt",
                Bytes::from("nested"),
                None,
                HashMap::new(),
            )
            .await
            .unwrap();

        let result = storage.get_object("b", "dir/subdir/file.txt").await.unwrap();
        assert_eq!(result.body, b"nested");
    }

    // --- ListObjectsV2 ---

    async fn setup_list_storage(tmp: &tempfile::TempDir) -> FsStorage {
        let storage = make_storage(tmp.path());
        storage.create_bucket("b").await.unwrap();
        for key in ["a.txt", "b.txt", "c.txt", "docs/x.txt", "docs/y.txt", "docs/sub/z.txt"] {
            storage
                .put_object("b", key, Bytes::from("data"), None, HashMap::new())
                .await
                .unwrap();
        }
        storage
    }

    #[tokio::test]
    async fn list_objects_no_params() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = setup_list_storage(&tmp).await;
        let params = ListObjectsV2Params {
            max_keys: 1000,
            ..Default::default()
        };
        let result = storage.list_objects_v2("b", &params).await.unwrap();
        assert_eq!(result.objects.len(), 6);
        assert!(!result.is_truncated);
    }

    #[tokio::test]
    async fn list_objects_prefix_filter() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = setup_list_storage(&tmp).await;
        let params = ListObjectsV2Params {
            prefix: Some("docs/".to_string()),
            max_keys: 1000,
            ..Default::default()
        };
        let result = storage.list_objects_v2("b", &params).await.unwrap();
        assert_eq!(result.objects.len(), 3);
        assert!(result.objects.iter().all(|o| o.key.starts_with("docs/")));
    }

    #[tokio::test]
    async fn list_objects_delimiter_common_prefixes() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = setup_list_storage(&tmp).await;
        let params = ListObjectsV2Params {
            delimiter: Some("/".to_string()),
            max_keys: 1000,
            ..Default::default()
        };
        let result = storage.list_objects_v2("b", &params).await.unwrap();
        // Root-level objects: a.txt, b.txt, c.txt
        assert_eq!(result.objects.len(), 3);
        // Common prefix: docs/
        assert_eq!(result.common_prefixes, vec!["docs/"]);
    }

    #[tokio::test]
    async fn list_objects_max_keys_truncation() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = setup_list_storage(&tmp).await;
        let params = ListObjectsV2Params {
            max_keys: 2,
            ..Default::default()
        };
        let result = storage.list_objects_v2("b", &params).await.unwrap();
        assert_eq!(result.objects.len(), 2);
        assert!(result.is_truncated);
        assert!(result.next_continuation_token.is_some());
    }

    #[tokio::test]
    async fn list_objects_start_after() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = setup_list_storage(&tmp).await;
        let params = ListObjectsV2Params {
            start_after: Some("b.txt".to_string()),
            max_keys: 1000,
            ..Default::default()
        };
        let result = storage.list_objects_v2("b", &params).await.unwrap();
        // Should skip a.txt and b.txt
        assert!(result.objects.iter().all(|o| o.key.as_str() > "b.txt"));
    }

    #[tokio::test]
    async fn list_objects_continuation_token_pagination() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = setup_list_storage(&tmp).await;

        // First page
        let params = ListObjectsV2Params {
            max_keys: 2,
            ..Default::default()
        };
        let page1 = storage.list_objects_v2("b", &params).await.unwrap();
        assert_eq!(page1.objects.len(), 2);
        assert!(page1.is_truncated);

        // Second page using continuation token
        let params = ListObjectsV2Params {
            max_keys: 2,
            continuation_token: page1.next_continuation_token.clone(),
            ..Default::default()
        };
        let page2 = storage.list_objects_v2("b", &params).await.unwrap();
        assert_eq!(page2.objects.len(), 2);

        // Keys from page2 should be after keys from page1
        let last_page1_key = &page1.objects.last().unwrap().key;
        let first_page2_key = &page2.objects.first().unwrap().key;
        assert!(first_page2_key > last_page1_key);
    }
}
