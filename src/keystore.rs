use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::types::{ApiKeyEntry, BucketPermission, BucketPolicy};

pub struct KeyStore {
    data_dir: Option<PathBuf>,
    keys: Vec<ApiKeyEntry>,
    policies: HashMap<String, BucketPolicy>,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct KeysFile {
    keys: Vec<ApiKeyEntry>,
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct PoliciesFile {
    policies: HashMap<String, BucketPolicy>,
}

impl KeyStore {
    /// Create an empty key store with no persistence (for when web auth is disabled)
    pub fn empty() -> Self {
        Self {
            data_dir: None,
            keys: Vec::new(),
            policies: HashMap::new(),
        }
    }

    /// Load key store from disk, or create empty if files don't exist
    pub fn load(data_dir: &Path) -> Self {
        let devfs_dir = data_dir.join(".devfs");
        std::fs::create_dir_all(&devfs_dir).ok();

        let keys = Self::load_keys(&devfs_dir);
        let policies = Self::load_policies(&devfs_dir);

        Self {
            data_dir: Some(devfs_dir),
            keys,
            policies,
        }
    }

    fn load_keys(devfs_dir: &Path) -> Vec<ApiKeyEntry> {
        let path = devfs_dir.join("keys.json");
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                serde_json::from_str::<KeysFile>(&content)
                    .map(|f| f.keys)
                    .unwrap_or_default()
            }
            Err(_) => Vec::new(),
        }
    }

    fn load_policies(devfs_dir: &Path) -> HashMap<String, BucketPolicy> {
        let path = devfs_dir.join("bucket_policies.json");
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                serde_json::from_str::<PoliciesFile>(&content)
                    .map(|f| f.policies)
                    .unwrap_or_default()
            }
            Err(_) => HashMap::new(),
        }
    }

    fn save(&self) {
        let Some(ref dir) = self.data_dir else { return };

        let keys_file = KeysFile {
            keys: self.keys.clone(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&keys_file) {
            std::fs::write(dir.join("keys.json"), json).ok();
        }

        let policies_file = PoliciesFile {
            policies: self.policies.clone(),
        };
        if let Ok(json) = serde_json::to_string_pretty(&policies_file) {
            std::fs::write(dir.join("bucket_policies.json"), json).ok();
        }
    }

    pub fn find_by_access_key(&self, ak: &str) -> Option<&ApiKeyEntry> {
        self.keys.iter().find(|k| k.access_key == ak)
    }

    pub fn create_key(&mut self, description: String, buckets: HashMap<String, BucketPermission>) -> ApiKeyEntry {
        let entry = ApiKeyEntry {
            id: uuid::Uuid::new_v4().to_string(),
            access_key: generate_access_key(),
            secret_key: generate_secret_key(),
            description,
            buckets,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        self.keys.push(entry.clone());
        self.save();
        entry
    }

    pub fn delete_key(&mut self, id: &str) -> bool {
        let len_before = self.keys.len();
        self.keys.retain(|k| k.id != id);
        let deleted = self.keys.len() < len_before;
        if deleted {
            self.save();
        }
        deleted
    }

    pub fn list_keys(&self) -> &[ApiKeyEntry] {
        &self.keys
    }

    pub fn update_key_buckets(&mut self, id: &str, buckets: HashMap<String, BucketPermission>, description: Option<String>) -> bool {
        if let Some(key) = self.keys.iter_mut().find(|k| k.id == id) {
            key.buckets = buckets;
            if let Some(desc) = description {
                key.description = desc;
            }
            self.save();
            true
        } else {
            false
        }
    }

    pub fn get_policy(&self, bucket: &str) -> BucketPolicy {
        self.policies.get(bucket).cloned().unwrap_or_default()
    }

    pub fn set_policy(&mut self, bucket: &str, policy: BucketPolicy) {
        self.policies.insert(bucket.to_string(), policy);
        self.save();
    }
}

fn generate_access_key() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let chars: Vec<char> = (0..20)
        .map(|_| {
            let idx = rng.gen_range(0..36);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'A' + idx - 10) as char
            }
        })
        .collect();
    chars.into_iter().collect()
}

fn generate_secret_key() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let chars: Vec<char> = (0..40)
        .map(|_| {
            let idx = rng.gen_range(0..62);
            if idx < 10 {
                (b'0' + idx) as char
            } else if idx < 36 {
                (b'A' + idx - 10) as char
            } else {
                (b'a' + idx - 36) as char
            }
        })
        .collect();
    chars.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_keystore() {
        let ks = KeyStore::empty();
        assert!(ks.list_keys().is_empty());
        assert!(ks.find_by_access_key("nonexistent").is_none());
    }

    #[test]
    fn create_and_find_key() {
        let mut ks = KeyStore::empty();
        let entry = ks.create_key("test key".to_string(), HashMap::new());
        assert!(!entry.access_key.is_empty());
        assert!(!entry.secret_key.is_empty());
        assert_eq!(entry.description, "test key");

        let found = ks.find_by_access_key(&entry.access_key).unwrap();
        assert_eq!(found.id, entry.id);
    }

    #[test]
    fn delete_key() {
        let mut ks = KeyStore::empty();
        let entry = ks.create_key("test".to_string(), HashMap::new());
        assert_eq!(ks.list_keys().len(), 1);
        assert!(ks.delete_key(&entry.id));
        assert!(ks.list_keys().is_empty());
        assert!(!ks.delete_key("nonexistent"));
    }

    #[test]
    fn update_key_buckets() {
        let mut ks = KeyStore::empty();
        let entry = ks.create_key("test".to_string(), HashMap::new());
        let mut buckets = HashMap::new();
        buckets.insert("my-bucket".to_string(), BucketPermission::Read);
        assert!(ks.update_key_buckets(&entry.id, buckets, None));

        let found = ks.find_by_access_key(&entry.access_key).unwrap();
        assert_eq!(found.buckets.get("my-bucket"), Some(&BucketPermission::Read));
    }

    #[test]
    fn bucket_policy_default() {
        let ks = KeyStore::empty();
        let policy = ks.get_policy("nonexistent");
        assert!(!policy.public_read);
        assert!(!policy.public_write);
    }

    #[test]
    fn set_bucket_policy() {
        let mut ks = KeyStore::empty();
        ks.set_policy("test-bucket", BucketPolicy { public_read: true, public_write: false });
        let policy = ks.get_policy("test-bucket");
        assert!(policy.public_read);
        assert!(!policy.public_write);
    }

    #[test]
    fn persistence_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let mut ks = KeyStore::load(dir.path());
        let entry = ks.create_key("persistent".to_string(), HashMap::new());
        ks.set_policy("bucket1", BucketPolicy { public_read: true, public_write: false });

        // Reload from disk
        let ks2 = KeyStore::load(dir.path());
        assert_eq!(ks2.list_keys().len(), 1);
        assert_eq!(ks2.find_by_access_key(&entry.access_key).unwrap().description, "persistent");
        assert!(ks2.get_policy("bucket1").public_read);
    }

    #[test]
    fn generated_keys_are_valid_length() {
        let ak = generate_access_key();
        let sk = generate_secret_key();
        assert_eq!(ak.len(), 20);
        assert_eq!(sk.len(), 40);
        assert!(ak.chars().all(|c| c.is_ascii_alphanumeric()));
        assert!(sk.chars().all(|c| c.is_ascii_alphanumeric()));
    }
}
