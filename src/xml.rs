use serde::Serialize;

pub const S3_XMLNS: &str = "http://s3.amazonaws.com/doc/2006-03-01/";

pub fn to_s3_xml<T: Serialize>(value: &T) -> Result<String, quick_xml::se::SeError> {
    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
    xml.push_str(&quick_xml::se::to_string(value)?);
    Ok(xml)
}

// --- XML response types ---

#[derive(Debug, Serialize)]
#[serde(rename = "ListAllMyBucketsResult")]
pub struct ListAllMyBucketsResult {
    #[serde(rename = "@xmlns")]
    pub xmlns: &'static str,
    #[serde(rename = "Owner")]
    pub owner: Owner,
    #[serde(rename = "Buckets")]
    pub buckets: Buckets,
}

#[derive(Debug, Serialize)]
pub struct Owner {
    #[serde(rename = "ID")]
    pub id: String,
    #[serde(rename = "DisplayName")]
    pub display_name: String,
}

#[derive(Debug, Serialize)]
pub struct Buckets {
    #[serde(rename = "Bucket", default)]
    pub bucket: Vec<BucketInfo>,
}

#[derive(Debug, Serialize)]
pub struct BucketInfo {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "CreationDate")]
    pub creation_date: String,
}

#[derive(Debug, Serialize)]
#[serde(rename = "ListBucketResult")]
pub struct ListBucketResult {
    #[serde(rename = "@xmlns")]
    pub xmlns: &'static str,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Prefix")]
    pub prefix: String,
    #[serde(rename = "Delimiter")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delimiter: Option<String>,
    #[serde(rename = "MaxKeys")]
    pub max_keys: u32,
    #[serde(rename = "KeyCount")]
    pub key_count: u32,
    #[serde(rename = "IsTruncated")]
    pub is_truncated: bool,
    #[serde(rename = "Contents")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub contents: Vec<ObjectInfo>,
    #[serde(rename = "CommonPrefixes")]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub common_prefixes: Vec<CommonPrefix>,
    #[serde(rename = "ContinuationToken")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub continuation_token: Option<String>,
    #[serde(rename = "NextContinuationToken")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_continuation_token: Option<String>,
    #[serde(rename = "StartAfter")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_after: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ObjectInfo {
    #[serde(rename = "Key")]
    pub key: String,
    #[serde(rename = "LastModified")]
    pub last_modified: String,
    #[serde(rename = "ETag")]
    pub etag: String,
    #[serde(rename = "Size")]
    pub size: u64,
    #[serde(rename = "StorageClass")]
    pub storage_class: String,
}

#[derive(Debug, Serialize)]
pub struct CommonPrefix {
    #[serde(rename = "Prefix")]
    pub prefix: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_s3_xml_list_all_my_buckets_result() {
        let result = ListAllMyBucketsResult {
            xmlns: S3_XMLNS,
            owner: Owner {
                id: "owner-id".to_string(),
                display_name: "owner".to_string(),
            },
            buckets: Buckets {
                bucket: vec![
                    BucketInfo {
                        name: "bucket-a".to_string(),
                        creation_date: "2024-01-01T00:00:00Z".to_string(),
                    },
                    BucketInfo {
                        name: "bucket-b".to_string(),
                        creation_date: "2024-01-02T00:00:00Z".to_string(),
                    },
                ],
            },
        };
        let xml = to_s3_xml(&result).unwrap();
        assert!(xml.starts_with("<?xml version=\"1.0\""));
        assert!(xml.contains("<Name>bucket-a</Name>"));
        assert!(xml.contains("<Name>bucket-b</Name>"));
        assert!(xml.contains("<DisplayName>owner</DisplayName>"));
    }

    #[test]
    fn to_s3_xml_list_all_my_buckets_result_empty() {
        let result = ListAllMyBucketsResult {
            xmlns: S3_XMLNS,
            owner: Owner {
                id: "id".to_string(),
                display_name: "name".to_string(),
            },
            buckets: Buckets { bucket: vec![] },
        };
        let xml = to_s3_xml(&result).unwrap();
        assert!(xml.starts_with("<?xml version=\"1.0\""));
        assert!(xml.contains("Buckets"));
    }

    #[test]
    fn to_s3_xml_list_bucket_result() {
        let result = ListBucketResult {
            xmlns: S3_XMLNS,
            name: "my-bucket".to_string(),
            prefix: "docs/".to_string(),
            delimiter: Some("/".to_string()),
            max_keys: 100,
            key_count: 2,
            is_truncated: true,
            contents: vec![ObjectInfo {
                key: "docs/file.txt".to_string(),
                last_modified: "2024-01-01T00:00:00Z".to_string(),
                etag: "\"abc123\"".to_string(),
                size: 42,
                storage_class: "STANDARD".to_string(),
            }],
            common_prefixes: vec![CommonPrefix {
                prefix: "docs/sub/".to_string(),
            }],
            continuation_token: None,
            next_continuation_token: Some("token123".to_string()),
            start_after: None,
        };
        let xml = to_s3_xml(&result).unwrap();
        assert!(xml.starts_with("<?xml version=\"1.0\""));
        assert!(xml.contains("<Name>my-bucket</Name>"));
        assert!(xml.contains("<Prefix>docs/</Prefix>"));
        assert!(xml.contains("<IsTruncated>true</IsTruncated>"));
        assert!(xml.contains("<Key>docs/file.txt</Key>"));
        assert!(xml.contains("<Size>42</Size>"));
        assert!(xml.contains("<NextContinuationToken>token123</NextContinuationToken>"));
        assert!(xml.contains("<Prefix>docs/sub/</Prefix>"));
    }
}
