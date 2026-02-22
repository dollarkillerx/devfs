use axum::extract::Request;
use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::error::S3Error;
use crate::types::{AppState, AuthConfig, BucketPermission, S3Operation};

type HmacSha256 = Hmac<Sha256>;

/// Verify an S3 request with support for admin keys, managed keys, and bucket policies.
pub fn verify_s3_request(
    request: &Request,
    state: &AppState,
    operation: &S3Operation,
) -> Result<(), S3Error> {
    let bucket = operation.bucket_name();

    // 1. Check bucket public access policy
    if let Some(b) = bucket {
        let ks = state.key_store.read().unwrap();
        let policy = ks.get_policy(b);
        if operation.is_read_only() && policy.public_read {
            return Ok(());
        }
        if !operation.is_read_only() && policy.public_write {
            return Ok(());
        }
    }

    // 2. No auth configured at all → allow everything
    if state.auth_config.is_none() && state.web_auth_config.is_none() {
        return Ok(());
    }

    // 3. Extract access_key from Authorization header
    let access_key = extract_access_key(request)?;

    // 4. Check admin key (from [auth] config)
    if let Some(ref admin) = state.auth_config {
        if access_key == admin.access_key {
            verify_signature(request, &admin.secret_key)?;
            return Ok(()); // Admin = full access
        }
    }

    // 5. Check managed keys (from key_store)
    let ks = state.key_store.read().unwrap();
    if let Some(entry) = ks.find_by_access_key(access_key) {
        verify_signature(request, &entry.secret_key)?;

        // Check per-bucket permission
        if let Some(b) = bucket {
            let perm = entry.buckets.get(b).unwrap_or(&BucketPermission::None);
            match perm {
                BucketPermission::ReadWrite => return Ok(()),
                BucketPermission::Read if operation.is_read_only() => return Ok(()),
                _ => return Err(S3Error::access_denied()),
            }
        }
        // No bucket (ListBuckets) — allow if key has any bucket permissions
        if !entry.buckets.is_empty() {
            return Ok(());
        }
        return Err(S3Error::access_denied());
    }

    Err(S3Error::access_denied())
}

/// Legacy single-key verification (kept for backward compat with tests)
pub fn verify_request(request: &Request, config: &AuthConfig) -> Result<(), S3Error> {
    let access_key = extract_access_key(request)?;
    if access_key != config.access_key {
        return Err(S3Error::access_denied());
    }
    verify_signature(request, &config.secret_key)
}

fn extract_access_key<'a>(request: &'a Request) -> Result<&'a str, S3Error> {
    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    let auth_str = match auth_header {
        Some(h) => h,
        None => return Err(S3Error::access_denied()),
    };

    if !auth_str.starts_with("AWS4-HMAC-SHA256") {
        return Err(S3Error::access_denied());
    }

    let credential = extract_field(auth_str, "Credential=")
        .ok_or_else(S3Error::access_denied)?;

    credential
        .split('/')
        .next()
        .ok_or_else(S3Error::access_denied)
}

fn canonicalize_query_string(query: Option<&str>) -> String {
    let Some(q) = query else {
        return String::new();
    };
    if q.is_empty() {
        return String::new();
    }
    let mut pairs: Vec<(&str, &str)> = q
        .split('&')
        .map(|p| p.split_once('=').unwrap_or((p, "")))
        .collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("&")
}

fn verify_signature(request: &Request, secret_key: &str) -> Result<(), S3Error> {
    let auth_str = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(S3Error::access_denied)?;

    let credential = extract_field(auth_str, "Credential=")
        .ok_or_else(S3Error::access_denied)?;
    let signed_headers = extract_field(auth_str, "SignedHeaders=")
        .ok_or_else(S3Error::access_denied)?;
    let provided_signature = extract_field(auth_str, "Signature=")
        .ok_or_else(S3Error::access_denied)?;

    // Extract date components from credential scope
    let scope_parts: Vec<&str> = credential.split('/').collect();
    if scope_parts.len() < 5 {
        return Err(S3Error::access_denied());
    }
    let date = scope_parts[1];
    let region = scope_parts[2];
    let service = scope_parts[3];

    // Build canonical request
    let method = request.method().as_str();
    let uri = request.uri().path();
    let query = canonicalize_query_string(request.uri().query());

    // Build canonical headers
    let header_names: Vec<&str> = signed_headers.split(';').collect();
    let mut canonical_headers = String::new();
    for name in &header_names {
        let value = request
            .headers()
            .get(*name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        canonical_headers.push_str(&format!("{}:{}\n", name, value.trim()));
    }

    // Payload hash — support UNSIGNED-PAYLOAD
    let payload_hash = request
        .headers()
        .get("x-amz-content-sha256")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("UNSIGNED-PAYLOAD");

    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        method, uri, query, canonical_headers, signed_headers, payload_hash
    );

    // String to sign
    let x_amz_date = request
        .headers()
        .get("x-amz-date")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let scope = format!("{}/{}/{}/aws4_request", date, region, service);
    let canonical_hash = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(
        canonical_request.as_bytes(),
    ));

    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        x_amz_date, scope, canonical_hash
    );

    // Derive signing key
    let signing_key = derive_signing_key(secret_key, date, region, service);

    // Calculate signature
    let mut mac =
        HmacSha256::new_from_slice(&signing_key).map_err(|_| S3Error::access_denied())?;
    mac.update(string_to_sign.as_bytes());
    let calculated = hex::encode(mac.finalize().into_bytes());

    if calculated != provided_signature {
        return Err(S3Error::access_denied());
    }

    Ok(())
}

fn derive_signing_key(secret_key: &str, date: &str, region: &str, service: &str) -> Vec<u8> {
    let k_date = hmac_sha256(format!("AWS4{}", secret_key).as_bytes(), date.as_bytes());
    let k_region = hmac_sha256(&k_date, region.as_bytes());
    let k_service = hmac_sha256(&k_region, service.as_bytes());
    hmac_sha256(&k_service, b"aws4_request")
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC key size");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn extract_field<'a>(header: &'a str, field: &str) -> Option<&'a str> {
    let start = header.find(field)? + field.len();
    let rest = &header[start..];
    let end = rest.find([',', ' ']).unwrap_or(rest.len());
    Some(&rest[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUTH_HEADER: &str = "AWS4-HMAC-SHA256 Credential=AKID/20120215/us-east-1/iam/aws4_request, SignedHeaders=content-type;host;x-amz-date, Signature=5d672d79c15b13162d9279b0855cfba6789a8edb4c82c400e06b5924a6f2b5d7";

    #[test]
    fn extract_field_credential() {
        let val = extract_field(AUTH_HEADER, "Credential=").unwrap();
        assert_eq!(val, "AKID/20120215/us-east-1/iam/aws4_request");
    }

    #[test]
    fn extract_field_signed_headers() {
        let val = extract_field(AUTH_HEADER, "SignedHeaders=").unwrap();
        assert_eq!(val, "content-type;host;x-amz-date");
    }

    #[test]
    fn extract_field_signature() {
        let val = extract_field(AUTH_HEADER, "Signature=").unwrap();
        assert_eq!(
            val,
            "5d672d79c15b13162d9279b0855cfba6789a8edb4c82c400e06b5924a6f2b5d7"
        );
    }

    #[test]
    fn extract_field_missing() {
        assert!(extract_field(AUTH_HEADER, "NotAField=").is_none());
    }

    #[test]
    fn hmac_sha256_known_value() {
        let result = hmac_sha256(b"key", b"data");
        let hex_str = hex::encode(&result);
        // Known HMAC-SHA256("key", "data")
        assert_eq!(
            hex_str,
            "5031fe3d989c6d1537a013fa6e739da23463fdaec3b70137d828e36ace221bd0"
        );
    }

    #[test]
    fn derive_signing_key_aws_test_vector() {
        // AWS Signature Version 4 Test Suite
        let key = derive_signing_key(
            "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY",
            "20120215",
            "us-east-1",
            "iam",
        );
        let hex_str = hex::encode(&key);
        assert_eq!(
            hex_str,
            "f4780e2d9f65fa895f9c67b32ce1baf0b0d8a43505a000a1a9e090d414db404d"
        );
    }

    #[test]
    fn canonicalize_query_string_sorts_params() {
        let result = canonicalize_query_string(Some("z=1&a=2&m=3"));
        assert_eq!(result, "a=2&m=3&z=1");
    }

    #[test]
    fn canonicalize_query_string_empty() {
        assert_eq!(canonicalize_query_string(None), "");
        assert_eq!(canonicalize_query_string(Some("")), "");
    }

    #[test]
    fn canonicalize_query_string_single_param() {
        assert_eq!(canonicalize_query_string(Some("key=val")), "key=val");
    }

    #[test]
    fn canonicalize_query_string_sorts_by_value_on_tie() {
        let result = canonicalize_query_string(Some("a=z&a=a"));
        assert_eq!(result, "a=a&a=z");
    }

    #[test]
    fn canonicalize_query_string_no_value() {
        let result = canonicalize_query_string(Some("b&a"));
        assert_eq!(result, "a=&b=");
    }
}
