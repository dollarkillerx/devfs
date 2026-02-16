# devfs

A simple S3-compatible file server for development workflows.

![](./image.png)

## Quick Start

Build and run:

```bash
cargo build --release
./target/release/devfs
```

The server starts on `http://127.0.0.1:9000` by default. Try it with the AWS CLI:

```bash
# Create a bucket
aws --endpoint-url http://127.0.0.1:9000 s3 mb s3://my-bucket

# Upload a file
aws --endpoint-url http://127.0.0.1:9000 s3 cp hello.txt s3://my-bucket/hello.txt

# Download a file
aws --endpoint-url http://127.0.0.1:9000 s3 cp s3://my-bucket/hello.txt downloaded.txt
```

## Configuration

Configuration is resolved in order: **CLI flags > environment variables > config file > defaults**.

| Parameter | CLI Flag | Environment Variable | TOML Key | Default |
|-----------|----------|---------------------|----------|---------|
| Host | `--host` | `DEVFS_HOST` | `host` | `127.0.0.1` |
| Port | `--port` | `DEVFS_PORT` | `port` | `9000` |
| Data directory | `--data-dir` | `DEVFS_DATA_DIR` | `data_dir` | `./data` |
| Access key | `--access-key` | `DEVFS_ACCESS_KEY` | `auth.access_key` | _(none)_ |
| Secret key | `--secret-key` | `DEVFS_SECRET_KEY` | `auth.secret_key` | _(none)_ |
| Config file | `--config` | — | — | `devfs.toml` |

Example `devfs.toml`:

```toml
host = "0.0.0.0"
port = 8080
data_dir = "/tmp/devfs"

[auth]
access_key = "mykey"
secret_key = "mysecret"
```

## Supported Operations

### Bucket Operations

| Operation | Method | Path |
|-----------|--------|------|
| ListBuckets | `GET /` | List all buckets |
| CreateBucket | `PUT /{bucket}` | Create a bucket |
| DeleteBucket | `DELETE /{bucket}` | Delete an empty bucket |
| HeadBucket | `HEAD /{bucket}` | Check if a bucket exists |

### Object Operations

| Operation | Method | Path |
|-----------|--------|------|
| ListObjectsV2 | `GET /{bucket}?list-type=2` | List objects in a bucket |
| PutObject | `PUT /{bucket}/{key}` | Upload an object |
| GetObject | `GET /{bucket}/{key}` | Download an object |
| DeleteObject | `DELETE /{bucket}/{key}` | Delete an object |
| HeadObject | `HEAD /{bucket}/{key}` | Get object metadata |

### ListObjectsV2 Query Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `prefix` | string | _(none)_ | Filter objects by key prefix |
| `delimiter` | string | _(none)_ | Group keys by delimiter (e.g. `/`) |
| `max-keys` | u32 | `1000` | Max number of keys to return |
| `start-after` | string | _(none)_ | Start listing after this key |
| `continuation-token` | string | _(none)_ | Resume from a previous response |

## Authentication

Authentication is **optional**. It is enabled when both `access_key` and `secret_key` are configured. When enabled, devfs validates AWS Signature Version 4 (SigV4) headers, so standard AWS SDKs and CLI tools work out of the box.

## Storage Layout

Objects are stored directly on the filesystem. Metadata is kept in `.meta/` directories alongside object data:

```
data/
├── my-bucket/
│   ├── .meta/
│   │   └── hello.txt.json      # metadata (content-type, etag, etc.)
│   ├── hello.txt               # object data
│   └── docs/
│       ├── .meta/
│       │   └── readme.txt.json
│       └── readme.txt
└── other-bucket/
    └── ...
```

## Benchmarks

Run with `CI=true cargo bench --features bench-internals`.

### Streaming Upload

| Size | Time | Throughput |
|------|------|------------|
| 1 KB | 107.6 µs | 9.08 MiB/s |
| 1 MB | 1.58 ms | 634 MiB/s |
| 10 MB | 14.0 ms | 712 MiB/s |

### Streaming Download

| Size | Time | Throughput |
|------|------|------------|
| 1 KB | 42.9 µs | 22.7 MiB/s |
| 1 MB | 80.8 µs | 12.1 GiB/s |
| 10 MB | 539 µs | 18.1 GiB/s |

### ListObjectsV2

| Scenario | 100 | 1,000 | 10,000 |
|----------|-----|-------|--------|
| flat | 1.31 ms | 17.0 ms | 184 ms |
| prefix_pruning | — | 1.18 ms | 18.9 ms |
| delimiter | — | 1.20 ms | 11.6 ms |

### ETag Consistency

| Size | Time |
|------|------|
| 1 KB | 211 µs |
| 1 MB | 2.85 ms |

## Project Structure

| File | LOC | Description |
|------|-----|-------------|
| `storage.rs` | 764 | Filesystem-backed object storage |
| `middleware.rs` | 317 | S3 request parsing and routing |
| `config.rs` | 208 | CLI, env, and TOML config loading |
| `auth.rs` | 196 | SigV4 signature verification |
| `xml.rs` | 182 | S3 XML response serialization |
| `error.rs` | 228 | S3 error codes and XML error responses |
| `dispatcher.rs` | 131 | Operation dispatch to handlers |
| `types.rs` | 59 | Shared types and data structures |
| `main.rs` | 45 | Server entrypoint |
