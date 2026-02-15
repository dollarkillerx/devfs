use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;

use crate::auth;
use crate::error::S3Error;
use crate::types::{AppState, ListObjectsV2Params, S3Operation};

pub async fn s3_operation_middleware(
    axum::extract::State(state): axum::extract::State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, S3Error> {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    let query = request.uri().query().map(|q| q.to_string());

    // Parse path segments, percent-decode each
    let segments: Vec<String> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| {
            percent_encoding::percent_decode_str(s)
                .decode_utf8_lossy()
                .to_string()
        })
        .collect();

    let operation = parse_operation(&method, &segments, query.as_deref())?;

    // Auth check
    if let Some(ref auth_config) = state.auth_config {
        auth::verify_request(&request, auth_config)?;
    }

    request.extensions_mut().insert(operation);
    Ok(next.run(request).await)
}

fn parse_operation(
    method: &axum::http::Method,
    segments: &[String],
    query: Option<&str>,
) -> Result<S3Operation, S3Error> {
    let query_params = parse_query(query);

    match (method.as_str(), segments.len()) {
        // GET / → ListBuckets
        ("GET", 0) => Ok(S3Operation::ListBuckets),

        // Bucket-level operations (1 segment)
        ("PUT", 1) => Ok(S3Operation::CreateBucket {
            bucket: segments[0].clone(),
        }),
        ("DELETE", 1) => Ok(S3Operation::DeleteBucket {
            bucket: segments[0].clone(),
        }),
        ("HEAD", 1) => Ok(S3Operation::HeadBucket {
            bucket: segments[0].clone(),
        }),
        ("GET", 1) => {
            // Check for list-type=2 query param
            let list_type = query_params.get("list-type").map(|s| s.as_str());
            if list_type == Some("2") {
                let params = ListObjectsV2Params {
                    prefix: query_params.get("prefix").cloned(),
                    delimiter: query_params.get("delimiter").cloned(),
                    max_keys: query_params
                        .get("max-keys")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(1000),
                    continuation_token: query_params.get("continuation-token").cloned(),
                    start_after: query_params.get("start-after").cloned(),
                };
                Ok(S3Operation::ListObjectsV2 {
                    bucket: segments[0].clone(),
                    params,
                })
            } else {
                // Treat any other GET on bucket as ListObjectsV2 (many S3 clients expect this)
                let params = ListObjectsV2Params {
                    prefix: query_params.get("prefix").cloned(),
                    delimiter: query_params.get("delimiter").cloned(),
                    max_keys: query_params
                        .get("max-keys")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(1000),
                    continuation_token: query_params.get("continuation-token").cloned(),
                    start_after: query_params.get("start-after").cloned(),
                };
                Ok(S3Operation::ListObjectsV2 {
                    bucket: segments[0].clone(),
                    params,
                })
            }
        }

        // Object-level operations (2+ segments)
        ("PUT", n) if n >= 2 => {
            let bucket = segments[0].clone();
            let key = segments[1..].join("/");
            Ok(S3Operation::PutObject { bucket, key })
        }
        ("GET", n) if n >= 2 => {
            let bucket = segments[0].clone();
            let key = segments[1..].join("/");
            Ok(S3Operation::GetObject { bucket, key })
        }
        ("DELETE", n) if n >= 2 => {
            let bucket = segments[0].clone();
            let key = segments[1..].join("/");
            Ok(S3Operation::DeleteObject { bucket, key })
        }
        ("HEAD", n) if n >= 2 => {
            let bucket = segments[0].clone();
            let key = segments[1..].join("/");
            Ok(S3Operation::HeadObject { bucket, key })
        }

        _ => Err(S3Error::new(
            crate::error::S3ErrorCode::InvalidArgument,
            format!("Unsupported operation: {} /{}", method, segments.join("/")),
            "",
        )),
    }
}

fn parse_query(query: Option<&str>) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    if let Some(q) = query {
        for pair in q.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                let key = percent_encoding::percent_decode_str(k)
                    .decode_utf8_lossy()
                    .to_string();
                let val = percent_encoding::percent_decode_str(v)
                    .decode_utf8_lossy()
                    .to_string();
                map.insert(key, val);
            } else {
                let key = percent_encoding::percent_decode_str(pair)
                    .decode_utf8_lossy()
                    .to_string();
                map.insert(key, String::new());
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Method;

    fn segs(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_op_list_buckets() {
        let op = parse_operation(&Method::GET, &segs(&[]), None).unwrap();
        assert!(matches!(op, S3Operation::ListBuckets));
    }

    #[test]
    fn parse_op_create_bucket() {
        let op = parse_operation(&Method::PUT, &segs(&["mybucket"]), None).unwrap();
        match op {
            S3Operation::CreateBucket { bucket } => assert_eq!(bucket, "mybucket"),
            _ => panic!("expected CreateBucket"),
        }
    }

    #[test]
    fn parse_op_delete_bucket() {
        let op = parse_operation(&Method::DELETE, &segs(&["mybucket"]), None).unwrap();
        match op {
            S3Operation::DeleteBucket { bucket } => assert_eq!(bucket, "mybucket"),
            _ => panic!("expected DeleteBucket"),
        }
    }

    #[test]
    fn parse_op_head_bucket() {
        let op = parse_operation(&Method::HEAD, &segs(&["mybucket"]), None).unwrap();
        match op {
            S3Operation::HeadBucket { bucket } => assert_eq!(bucket, "mybucket"),
            _ => panic!("expected HeadBucket"),
        }
    }

    #[test]
    fn parse_op_list_objects_v2_default() {
        let op = parse_operation(&Method::GET, &segs(&["mybucket"]), None).unwrap();
        match op {
            S3Operation::ListObjectsV2 { bucket, params } => {
                assert_eq!(bucket, "mybucket");
                assert_eq!(params.max_keys, 1000);
                assert!(params.prefix.is_none());
                assert!(params.delimiter.is_none());
            }
            _ => panic!("expected ListObjectsV2"),
        }
    }

    #[test]
    fn parse_op_list_objects_v2_with_params() {
        let op = parse_operation(
            &Method::GET,
            &segs(&["mybucket"]),
            Some("list-type=2&prefix=foo/&delimiter=/&max-keys=10"),
        )
        .unwrap();
        match op {
            S3Operation::ListObjectsV2 { bucket, params } => {
                assert_eq!(bucket, "mybucket");
                assert_eq!(params.prefix.as_deref(), Some("foo/"));
                assert_eq!(params.delimiter.as_deref(), Some("/"));
                assert_eq!(params.max_keys, 10);
            }
            _ => panic!("expected ListObjectsV2"),
        }
    }

    #[test]
    fn parse_op_put_object() {
        let op = parse_operation(&Method::PUT, &segs(&["mybucket", "mykey"]), None).unwrap();
        match op {
            S3Operation::PutObject { bucket, key } => {
                assert_eq!(bucket, "mybucket");
                assert_eq!(key, "mykey");
            }
            _ => panic!("expected PutObject"),
        }
    }

    #[test]
    fn parse_op_get_object_multi_segment_key() {
        let op = parse_operation(
            &Method::GET,
            &segs(&["mybucket", "dir", "subdir", "file.txt"]),
            None,
        )
        .unwrap();
        match op {
            S3Operation::GetObject { bucket, key } => {
                assert_eq!(bucket, "mybucket");
                assert_eq!(key, "dir/subdir/file.txt");
            }
            _ => panic!("expected GetObject"),
        }
    }

    #[test]
    fn parse_op_delete_object() {
        let op =
            parse_operation(&Method::DELETE, &segs(&["mybucket", "mykey"]), None).unwrap();
        match op {
            S3Operation::DeleteObject { bucket, key } => {
                assert_eq!(bucket, "mybucket");
                assert_eq!(key, "mykey");
            }
            _ => panic!("expected DeleteObject"),
        }
    }

    #[test]
    fn parse_op_head_object() {
        let op = parse_operation(&Method::HEAD, &segs(&["mybucket", "mykey"]), None).unwrap();
        match op {
            S3Operation::HeadObject { bucket, key } => {
                assert_eq!(bucket, "mybucket");
                assert_eq!(key, "mykey");
            }
            _ => panic!("expected HeadObject"),
        }
    }

    #[test]
    fn parse_op_unsupported_method() {
        let result = parse_operation(&Method::PATCH, &segs(&["mybucket"]), None);
        assert!(result.is_err());
    }

    #[test]
    fn parse_query_none() {
        let map = parse_query(None);
        assert!(map.is_empty());
    }

    #[test]
    fn parse_query_basic() {
        let map = parse_query(Some("key=value"));
        assert_eq!(map.get("key").unwrap(), "value");
    }

    #[test]
    fn parse_query_multiple() {
        let map = parse_query(Some("a=1&b=2&c=3"));
        assert_eq!(map.get("a").unwrap(), "1");
        assert_eq!(map.get("b").unwrap(), "2");
        assert_eq!(map.get("c").unwrap(), "3");
    }

    #[test]
    fn parse_query_percent_encoded() {
        let map = parse_query(Some("key=hello%20world"));
        assert_eq!(map.get("key").unwrap(), "hello world");
    }

    #[test]
    fn parse_query_key_without_value() {
        let map = parse_query(Some("flag"));
        assert_eq!(map.get("flag").unwrap(), "");
    }
}
