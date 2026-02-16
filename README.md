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
| Web user | `--web-user` | `DEVFS_WEB_USER` | `auth.web.user` | _(none)_ |
| Web password | `--web-password` | `DEVFS_WEB_PASSWORD` | `auth.web.password` | _(none)_ |
| Config file | `--config` | — | — | `devfs.toml` |

Example `devfs.toml`:

```toml
host = "0.0.0.0"
port = 8080
data_dir = "/tmp/devfs"

[auth]
access_key = "mykey"
secret_key = "mysecret"

[auth.web]
user = "admin"
password = "secretpass"
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

Authentication is **optional** and depends on which credentials are configured:

| auth keys | auth.web | Behavior |
|-----------|----------|----------|
| empty | not set | No auth — all S3 requests allowed |
| set | not set | Single admin key, SigV4 required, all buckets |
| empty | set | Web UI only, managed API keys with per-bucket permissions |
| set | set | Admin key (full access) + managed keys with per-bucket permissions |

When auth keys are configured, devfs validates AWS Signature Version 4 (SigV4) headers, so standard AWS SDKs and CLI tools work out of the box. Empty strings are treated as unset.

Bucket policies (`public_read`, `public_write`) can allow unauthenticated access to specific buckets even when authentication is enabled.

## Web Management UI

Enable the web UI by configuring `[auth.web]` with a username and password. Once enabled, the UI is available at `http://host:port/_web/`.

Features:
- Bucket management (create, delete)
- Object browser with upload and download
- API key management (create, revoke)
- Per-bucket permission assignment for API keys

Sessions are cookie-based with a 24-hour TTL and stored in memory (lost on server restart).

## API Keys & Permissions

When the web UI is enabled, you can create and manage multiple API keys through it. Each key can be granted per-bucket permission levels:

- **none** — no access (default)
- **read** — GetObject, HeadObject, ListObjectsV2
- **read_write** — full read + PutObject, DeleteObject

Key data is persisted in `{data_dir}/.devfs/keys.json`. Bucket policies are stored in `{data_dir}/.devfs/bucket_policies.json`.

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
| `storage.rs` | 1044 | Filesystem-backed object storage |
| `web/api.rs` | 425 | Web management REST API handlers |
| `middleware.rs` | 323 | S3 request parsing and routing |
| `config.rs` | 308 | CLI, env, and TOML config loading |
| `auth.rs` | 266 | SigV4 signature verification + multi-key auth |
| `keystore.rs` | 260 | API key and bucket policy persistence |
| `error.rs` | 228 | S3 error codes and XML error responses |
| `xml.rs` | 182 | S3 XML response serialization |
| `types.rs` | 135 | Shared types and data structures |
| `dispatcher.rs` | 134 | Operation dispatch to handlers |
| `session.rs` | 72 | In-memory session management |
| `web/mod.rs` | 52 | Web UI router and session middleware |
| `main.rs` | 47 | Server entrypoint |
| `web/static_files.rs` | 37 | Embedded static file serving |
