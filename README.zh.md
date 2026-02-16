# devfs

[English](./README.md) | [中文](./README.zh.md) | [日本語](./README.ja.md)

一个简单的、兼容 S3 的文件服务器，专为开发工作流设计。

![](./image.png)

## 快速开始

使用 Docker Compose 启动 devfs：

```bash
docker compose up -d
```

通过项目根目录的 `.env` 文件进行配置：

```env
DEVFS_PORT=9000
DEVFS_ACCESS_KEY=mykey
DEVFS_SECRET_KEY=mysecret
DEVFS_WEB_USER=admin
DEVFS_WEB_PASSWORD=secretpass
```

停止服务：

```bash
docker compose down
```

服务器默认在 `http://127.0.0.1:9000` 上启动。可以使用 AWS CLI 进行测试：

```bash
# 创建存储桶
aws --endpoint-url http://127.0.0.1:9000 s3 mb s3://my-bucket

# 上传文件
aws --endpoint-url http://127.0.0.1:9000 s3 cp hello.txt s3://my-bucket/hello.txt

# 下载文件
aws --endpoint-url http://127.0.0.1:9000 s3 cp s3://my-bucket/hello.txt downloaded.txt
```

### Python (boto3)

```python
import boto3

s3 = boto3.client(
    "s3",
    endpoint_url="http://127.0.0.1:9000",
    aws_access_key_id="mykey",
    aws_secret_access_key="mysecret",
    region_name="us-east-1",
)

# 创建存储桶
s3.create_bucket(Bucket="my-bucket")

# 上传
s3.put_object(Bucket="my-bucket", Key="hello.txt", Body=b"Hello, devfs!")

# 下载
resp = s3.get_object(Bucket="my-bucket", Key="hello.txt")
print(resp["Body"].read().decode())

# 列出对象
for obj in s3.list_objects_v2(Bucket="my-bucket").get("Contents", []):
    print(obj["Key"], obj["Size"])

# 删除
s3.delete_object(Bucket="my-bucket", Key="hello.txt")
```

### Go (aws-sdk-go-v2)

```go
package main

import (
	"context"
	"fmt"
	"io"
	"strings"

	"github.com/aws/aws-sdk-go-v2/aws"
	"github.com/aws/aws-sdk-go-v2/config"
	"github.com/aws/aws-sdk-go-v2/credentials"
	"github.com/aws/aws-sdk-go-v2/service/s3"
)

func main() {
	cfg, _ := config.LoadDefaultConfig(context.TODO(),
		config.WithRegion("us-east-1"),
		config.WithCredentialsProvider(
			credentials.NewStaticCredentialsProvider("mykey", "mysecret", ""),
		),
	)
	client := s3.NewFromConfig(cfg, func(o *s3.Options) {
		o.BaseEndpoint = aws.String("http://127.0.0.1:9000")
		o.UsePathStyle = true
	})

	ctx := context.TODO()

	// Create a bucket
	client.CreateBucket(ctx, &s3.CreateBucketInput{Bucket: aws.String("my-bucket")})

	// Upload
	client.PutObject(ctx, &s3.PutObjectInput{
		Bucket: aws.String("my-bucket"),
		Key:    aws.String("hello.txt"),
		Body:   strings.NewReader("Hello, devfs!"),
	})

	// Download
	out, _ := client.GetObject(ctx, &s3.GetObjectInput{
		Bucket: aws.String("my-bucket"),
		Key:    aws.String("hello.txt"),
	})
	body, _ := io.ReadAll(out.Body)
	fmt.Println(string(body))

	// List objects
	list, _ := client.ListObjectsV2(ctx, &s3.ListObjectsV2Input{
		Bucket: aws.String("my-bucket"),
	})
	for _, obj := range list.Contents {
		fmt.Printf("%s %d\n", *obj.Key, obj.Size)
	}

	// Delete
	client.DeleteObject(ctx, &s3.DeleteObjectInput{
		Bucket: aws.String("my-bucket"),
		Key:    aws.String("hello.txt"),
	})
}
```

## 配置

配置的优先级顺序为：**CLI 标志 > 环境变量 > 配置文件 > 默认值**。

| 参数 | CLI 标志 | 环境变量 | TOML 键 | 默认值 |
|------|----------|----------|---------|--------|
| 主机 | `--host` | `DEVFS_HOST` | `host` | `127.0.0.1` |
| 端口 | `--port` | `DEVFS_PORT` | `port` | `9000` |
| 数据目录 | `--data-dir` | `DEVFS_DATA_DIR` | `data_dir` | `./data` |
| Access Key | `--access-key` | `DEVFS_ACCESS_KEY` | `auth.access_key` | _(无)_ |
| Secret Key | `--secret-key` | `DEVFS_SECRET_KEY` | `auth.secret_key` | _(无)_ |
| Web 用户名 | `--web-user` | `DEVFS_WEB_USER` | `auth.web.user` | _(无)_ |
| Web 密码 | `--web-password` | `DEVFS_WEB_PASSWORD` | `auth.web.password` | _(无)_ |
| 配置文件 | `--config` | — | — | `devfs.toml` |

`devfs.toml` 示例：

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

## 支持的操作

### 存储桶操作

| 操作 | 方法 | 路径 |
|------|------|------|
| ListBuckets | `GET /` | 列出所有存储桶 |
| CreateBucket | `PUT /{bucket}` | 创建存储桶 |
| DeleteBucket | `DELETE /{bucket}` | 删除空存储桶 |
| HeadBucket | `HEAD /{bucket}` | 检查存储桶是否存在 |

### 对象操作

| 操作 | 方法 | 路径 |
|------|------|------|
| ListObjectsV2 | `GET /{bucket}?list-type=2` | 列出存储桶中的对象 |
| PutObject | `PUT /{bucket}/{key}` | 上传对象 |
| GetObject | `GET /{bucket}/{key}` | 下载对象 |
| DeleteObject | `DELETE /{bucket}/{key}` | 删除对象 |
| HeadObject | `HEAD /{bucket}/{key}` | 获取对象元数据 |

### ListObjectsV2 查询参数

| 参数 | 类型 | 默认值 | 描述 |
|------|------|--------|------|
| `prefix` | string | _(无)_ | 按键前缀过滤对象 |
| `delimiter` | string | _(无)_ | 按分隔符分组键（如 `/`） |
| `max-keys` | u32 | `1000` | 返回的最大键数 |
| `start-after` | string | _(无)_ | 从此键之后开始列出 |
| `continuation-token` | string | _(无)_ | 从上一次响应继续 |

## 认证

认证是**可选的**，取决于配置了哪些凭据：

| auth keys | auth.web | 行为 |
|-----------|----------|------|
| 空 | 未设置 | 无认证 — 所有 S3 请求均允许 |
| 已设置 | 未设置 | 单一管理员密钥，需要 SigV4，所有存储桶 |
| 空 | 已设置 | 仅 Web UI，托管 API 密钥，按存储桶分配权限 |
| 已设置 | 已设置 | 管理员密钥（完全访问）+ 托管密钥，按存储桶分配权限 |

配置了 auth keys 后，devfs 会验证 AWS Signature Version 4 (SigV4) 请求头，因此标准 AWS SDK 和 CLI 工具可以直接使用。空字符串视为未设置。

存储桶策略（`public_read`、`public_write`）可以在启用认证的情况下，允许对特定存储桶进行未认证访问。

## Web 管理界面

通过 `devfs.toml` 中的 `[auth.web]`、CLI 标志或环境变量（`DEVFS_WEB_USER` / `DEVFS_WEB_PASSWORD`）设置用户名和密码来启用 Web UI。启用后可在以下地址访问：

```
http://host:port/_web/
```

### 功能

- **存储桶管理** — 创建和删除存储桶
- **对象浏览器** — 在各存储桶内上传、下载和删除对象
- **API 密钥管理** — 创建和撤销 access/secret 密钥对
- **按存储桶权限** — 为每个密钥分配每个存储桶的 `read` 或 `read_write` 权限

### 典型工作流

1. 打开 `http://127.0.0.1:9000/_web/` 并使用配置的凭据登录
2. 在仪表板中创建存储桶
3. 进入存储桶并上传文件
4. 在 **API Keys** 下创建 API 密钥
5. 为密钥分配按存储桶的权限（如为 `my-bucket` 设置 `read_write`）
6. 在任何 S3 客户端中使用生成的 access/secret 密钥对

会话基于 Cookie，TTL 为 24 小时，存储在内存中（服务器重启后丢失）。

## API 密钥与权限

启用 Web UI 后，可以通过界面创建和管理多个 API 密钥。每个密钥可被授予按存储桶的权限级别：

- **none** — 无访问权限（默认）
- **read** — GetObject、HeadObject、ListObjectsV2
- **read_write** — 完全读取 + PutObject、DeleteObject

密钥数据持久化存储在 `{data_dir}/.devfs/keys.json`。存储桶策略存储在 `{data_dir}/.devfs/bucket_policies.json`。

## 存储布局

对象直接存储在文件系统上。元数据保存在对象数据旁的 `.meta/` 目录中：

```
data/
├── my-bucket/
│   ├── .meta/
│   │   └── hello.txt.json      # 元数据（content-type、etag 等）
│   ├── hello.txt               # 对象数据
│   └── docs/
│       ├── .meta/
│       │   └── readme.txt.json
│       └── readme.txt
└── other-bucket/
    └── ...
```

## 基准测试

运行命令：`CI=true cargo bench --features bench-internals`。

### 流式上传

| 大小 | 时间 | 吞吐量 |
|------|------|--------|
| 1 KB | 107.6 µs | 9.08 MiB/s |
| 1 MB | 1.58 ms | 634 MiB/s |
| 10 MB | 14.0 ms | 712 MiB/s |

### 流式下载

| 大小 | 时间 | 吞吐量 |
|------|------|--------|
| 1 KB | 42.9 µs | 22.7 MiB/s |
| 1 MB | 80.8 µs | 12.1 GiB/s |
| 10 MB | 539 µs | 18.1 GiB/s |

### ListObjectsV2

| 场景 | 100 | 1,000 | 10,000 |
|------|-----|-------|--------|
| flat | 1.31 ms | 17.0 ms | 184 ms |
| prefix_pruning | — | 1.18 ms | 18.9 ms |
| delimiter | — | 1.20 ms | 11.6 ms |

### ETag 一致性

| 大小 | 时间 |
|------|------|
| 1 KB | 211 µs |
| 1 MB | 2.85 ms |

## 项目结构

| 文件 | 行数 | 描述 |
|------|------|------|
| `storage.rs` | 1044 | 基于文件系统的对象存储 |
| `web/api.rs` | 425 | Web 管理 REST API 处理器 |
| `middleware.rs` | 323 | S3 请求解析与路由 |
| `config.rs` | 308 | CLI、环境变量和 TOML 配置加载 |
| `auth.rs` | 266 | SigV4 签名验证 + 多密钥认证 |
| `keystore.rs` | 260 | API 密钥和存储桶策略持久化 |
| `error.rs` | 228 | S3 错误码和 XML 错误响应 |
| `xml.rs` | 182 | S3 XML 响应序列化 |
| `types.rs` | 135 | 共享类型和数据结构 |
| `dispatcher.rs` | 134 | 操作分发到处理器 |
| `session.rs` | 72 | 内存会话管理 |
| `web/mod.rs` | 52 | Web UI 路由和会话中间件 |
| `main.rs` | 47 | 服务器入口 |
| `web/static_files.rs` | 37 | 嵌入式静态文件服务 |
