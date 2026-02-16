use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use chrono::Utc;
use http_body_util::BodyExt;
use md5::{Digest, Md5};
use tokio::fs;
use tokio::io::AsyncWriteExt;

use crate::error::S3Error;
use crate::types::{ListObjectsV2Params, ObjectMetadata};

#[derive(Clone)]
pub struct FsStorage {
    root: PathBuf,
}

#[cfg(any(test, feature = "bench-internals"))]
#[derive(Debug)]
pub struct GetObjectResult {
    pub metadata: ObjectMetadata,
    pub body: Vec<u8>,
}

pub struct GetObjectStreamResult {
    pub metadata: ObjectMetadata,
    pub file: tokio::fs::File,
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

    #[cfg(any(test, feature = "bench-internals"))]
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

    pub async fn put_object_stream(
        &self,
        bucket: &str,
        key: &str,
        body: axum::body::Body,
        content_type: Option<String>,
        custom_metadata: HashMap<String, String>,
        _content_length: Option<u64>,
    ) -> Result<String, S3Error> {
        let bucket_path = self.bucket_path(bucket);
        if !bucket_path.exists() {
            return Err(S3Error::no_such_bucket(bucket));
        }

        // Create object file
        let obj_path = self.object_path(bucket, key);
        if let Some(parent) = obj_path.parent() {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| S3Error::internal(format!("Failed to create dirs: {}", e)))?;
        }

        let mut file = fs::File::create(&obj_path)
            .await
            .map_err(|e| S3Error::internal(format!("Failed to create file: {}", e)))?;

        // Stream body frames to disk, computing MD5 incrementally
        let mut hasher = Md5::new();
        let mut total_bytes: u64 = 0;
        let mut body = body;

        loop {
            match body.frame().await {
                Some(Ok(frame)) => {
                    if let Ok(data) = frame.into_data() {
                        hasher.update(&data);
                        total_bytes += data.len() as u64;
                        if let Err(e) = file.write_all(&data).await {
                            // Clean up partial file on write error
                            drop(file);
                            let _ = fs::remove_file(&obj_path).await;
                            return Err(S3Error::internal(format!("Failed to write file: {}", e)));
                        }
                    }
                }
                Some(Err(e)) => {
                    // Clean up partial file on stream error
                    drop(file);
                    let _ = fs::remove_file(&obj_path).await;
                    return Err(S3Error::internal(format!("Failed to read body: {}", e)));
                }
                None => break,
            }
        }

        file.flush()
            .await
            .map_err(|e| S3Error::internal(format!("Failed to flush file: {}", e)))?;

        let etag = format!("\"{}\"", hex::encode(hasher.finalize()));

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
            content_length: total_bytes,
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

    #[cfg(any(test, feature = "bench-internals"))]
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

    pub async fn get_object_stream(
        &self,
        bucket: &str,
        key: &str,
    ) -> Result<GetObjectStreamResult, S3Error> {
        let bucket_path = self.bucket_path(bucket);
        if !bucket_path.exists() {
            return Err(S3Error::no_such_bucket(bucket));
        }

        let obj_path = self.object_path(bucket, key);
        if !obj_path.exists() || obj_path.is_dir() {
            return Err(S3Error::no_such_key(key));
        }

        let metadata = self.read_metadata(bucket, key).await?;
        let file = fs::File::open(&obj_path)
            .await
            .map_err(|e| S3Error::internal(format!("Failed to open object: {}", e)))?;

        Ok(GetObjectStreamResult { metadata, file })
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
        let start_after = params
            .continuation_token
            .as_deref()
            .or(params.start_after.as_deref());

        // Handle delimiter grouping
        if let Some(delimiter) = &params.delimiter {
            // With delimiter, we need all matching keys to correctly compute
            // common prefixes (can't predict folding for early termination).
            // But we still benefit from prefix pruning.
            let mut all_keys = Vec::new();
            self.collect_keys_sorted(&bucket_path, &bucket_path, prefix, start_after, usize::MAX, &mut all_keys)
                .await?;

            let mut objects = Vec::new();
            let mut common_prefixes = BTreeSet::new();

            for key in &all_keys {
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

            // Collect keys that need metadata
            let mut keys_needing_meta = Vec::new();
            while count < max_keys && (obj_idx < objects.len() || cp_idx < cp_vec.len()) {
                let use_obj = if obj_idx < objects.len() && cp_idx < cp_vec.len() {
                    objects[obj_idx] < cp_vec[cp_idx]
                } else {
                    obj_idx < objects.len()
                };

                if use_obj {
                    keys_needing_meta.push((result_objects.len(), objects[obj_idx].clone()));
                    last_key = Some(objects[obj_idx].clone());
                    // Push placeholder, will be filled by concurrent read
                    result_objects.push(ObjectEntry {
                        key: objects[obj_idx].clone(),
                        size: 0,
                        etag: String::new(),
                        last_modified: String::new(),
                    });
                    obj_idx += 1;
                } else {
                    last_key = Some(cp_vec[cp_idx].clone());
                    result_prefixes.push(cp_vec[cp_idx].clone());
                    cp_idx += 1;
                }
                count += 1;
            }

            // Concurrent metadata reads
            let mut set = tokio::task::JoinSet::new();
            for (idx, key) in keys_needing_meta {
                let storage = self.clone();
                let bucket = bucket.to_string();
                set.spawn(async move {
                    let meta = storage.read_metadata(&bucket, &key).await?;
                    Ok::<_, S3Error>((idx, key, meta))
                });
            }
            while let Some(res) = set.join_next().await {
                let (idx, key, meta) = res
                    .map_err(|e| S3Error::internal(format!("Task join error: {}", e)))??;
                result_objects[idx] = ObjectEntry {
                    key,
                    size: meta.content_length,
                    etag: meta.etag,
                    last_modified: meta.last_modified,
                };
            }

            Ok(ListObjectsV2Result {
                objects: result_objects,
                common_prefixes: result_prefixes,
                is_truncated,
                next_continuation_token: if is_truncated { last_key } else { None },
            })
        } else {
            // No delimiter — flat listing with early termination
            let keys = self
                .collect_keys_filtered(&bucket_path, prefix, start_after, max_keys as usize)
                .await?;

            let is_truncated = keys.len() > max_keys as usize;
            let keys: Vec<String> = keys.into_iter().take(max_keys as usize).collect();
            let last_key = keys.last().cloned();

            // Concurrent metadata reads
            let num_keys = keys.len();
            let mut result_objects: Vec<Option<ObjectEntry>> = (0..num_keys).map(|_| None).collect();

            let mut set = tokio::task::JoinSet::new();
            for (idx, key) in keys.into_iter().enumerate() {
                let storage = self.clone();
                let bucket = bucket.to_string();
                set.spawn(async move {
                    let meta = storage.read_metadata(&bucket, &key).await?;
                    Ok::<_, S3Error>((idx, key, meta))
                });
            }
            while let Some(res) = set.join_next().await {
                let (idx, key, meta) = res
                    .map_err(|e| S3Error::internal(format!("Task join error: {}", e)))??;
                result_objects[idx] = Some(ObjectEntry {
                    key,
                    size: meta.content_length,
                    etag: meta.etag,
                    last_modified: meta.last_modified,
                });
            }

            Ok(ListObjectsV2Result {
                objects: result_objects.into_iter().flatten().collect(),
                common_prefixes: Vec::new(),
                is_truncated,
                next_continuation_token: if is_truncated { last_key } else { None },
            })
        }
    }

    /// Collect keys filtered by prefix and start_after, returning at most `limit + 1` keys.
    /// The extra key is used to detect truncation.
    async fn collect_keys_filtered(
        &self,
        bucket_path: &Path,
        prefix: &str,
        start_after: Option<&str>,
        limit: usize,
    ) -> Result<Vec<String>, S3Error> {
        let mut keys = Vec::new();
        self.collect_keys_sorted(bucket_path, bucket_path, prefix, start_after, limit + 1, &mut keys)
            .await?;
        Ok(keys)
    }

    /// Recursively collect keys in sorted order with prefix pruning and early termination.
    /// Reads directory entries sorted by name, skips directories that can't contain matching keys,
    /// and stops once `limit` keys have been collected.
    async fn collect_keys_sorted(
        &self,
        bucket_path: &Path,
        dir: &Path,
        prefix: &str,
        start_after: Option<&str>,
        limit: usize,
        result: &mut Vec<String>,
    ) -> Result<(), S3Error> {
        if result.len() >= limit {
            return Ok(());
        }

        let mut entries = match fs::read_dir(dir).await {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };

        // Collect and sort directory entries by name
        let mut dir_entries = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| S3Error::internal(e.to_string()))?
        {
            let name = entry.file_name().to_string_lossy().to_string();
            if name == ".meta" {
                continue;
            }
            dir_entries.push(entry);
        }
        dir_entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

        for entry in dir_entries {
            if result.len() >= limit {
                return Ok(());
            }

            let path = entry.path();
            let ft = entry
                .file_type()
                .await
                .map_err(|e| S3Error::internal(e.to_string()))?;

            if ft.is_dir() {
                // Compute the directory's prefix relative to bucket root
                let dir_prefix = format!(
                    "{}/",
                    path.strip_prefix(bucket_path)
                        .unwrap()
                        .to_string_lossy()
                );

                // Prefix pruning: skip directories that can't contain matching keys
                if !prefix.is_empty()
                    && !dir_prefix.starts_with(prefix)
                    && !prefix.starts_with(&dir_prefix)
                {
                    continue;
                }

                Box::pin(self.collect_keys_sorted(
                    bucket_path,
                    &path,
                    prefix,
                    start_after,
                    limit,
                    result,
                ))
                .await?;
            } else {
                let key = path
                    .strip_prefix(bucket_path)
                    .unwrap()
                    .to_string_lossy()
                    .to_string();

                // Apply prefix filter
                if !key.starts_with(prefix) {
                    continue;
                }

                // Apply start_after filter
                if let Some(start) = start_after {
                    if key.as_str() <= start {
                        continue;
                    }
                }

                result.push(key);
            }
        }

        Ok(())
    }

    async fn read_metadata(&self, bucket: &str, key: &str) -> Result<ObjectMetadata, S3Error> {
        let meta_path = self.meta_path(bucket, key);
        let data = fs::read_to_string(&meta_path)
            .await
            .map_err(|_| S3Error::no_such_key(key))?;
        serde_json::from_str(&data)
            .map_err(|e| S3Error::internal(format!("Corrupt metadata for {}: {}", key, e)))
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
    async fn put_object_stream_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = make_storage(tmp.path());
        storage.create_bucket("b").await.unwrap();

        let body = axum::body::Body::from("streamed upload data");
        let etag = storage
            .put_object_stream(
                "b",
                "uploaded.txt",
                body,
                Some("text/plain".to_string()),
                HashMap::new(),
                None,
            )
            .await
            .unwrap();

        // Verify via get_object
        let result = storage.get_object("b", "uploaded.txt").await.unwrap();
        assert_eq!(result.body, b"streamed upload data");
        assert_eq!(result.metadata.content_type, "text/plain");
        assert_eq!(result.metadata.etag, etag);
        assert_eq!(result.metadata.content_length, 20);
    }

    #[tokio::test]
    async fn get_object_stream_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let storage = make_storage(tmp.path());
        storage.create_bucket("b").await.unwrap();
        storage
            .put_object(
                "b",
                "stream.txt",
                Bytes::from("streaming content"),
                Some("text/plain".to_string()),
                HashMap::new(),
            )
            .await
            .unwrap();

        let result = storage.get_object_stream("b", "stream.txt").await.unwrap();
        assert_eq!(result.metadata.content_type, "text/plain");
        assert_eq!(result.metadata.content_length, 17);

        // Read the file to verify contents
        use tokio::io::AsyncReadExt;
        let mut buf = Vec::new();
        let mut file = result.file;
        file.read_to_end(&mut buf).await.unwrap();
        assert_eq!(buf, b"streaming content");
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
