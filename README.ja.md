# devfs

[English](./README.md) | [中文](./README.zh.md) | [日本語](./README.ja.md)

開発ワークフロー向けのシンプルな S3 互換ファイルサーバーです。

![](./image.png)

## クイックスタート

Docker Compose で devfs を起動します：

```bash
docker compose up -d
```

プロジェクトルートの `.env` ファイルで設定します：

```env
DEVFS_PORT=9000
DEVFS_ACCESS_KEY=mykey
DEVFS_SECRET_KEY=mysecret
DEVFS_WEB_USER=admin
DEVFS_WEB_PASSWORD=secretpass
```

サービスを停止します：

```bash
docker compose down
```

サーバーはデフォルトで `http://127.0.0.1:9000` で起動します。AWS CLI で試してみましょう：

```bash
# バケットを作成
aws --endpoint-url http://127.0.0.1:9000 s3 mb s3://my-bucket

# ファイルをアップロード
aws --endpoint-url http://127.0.0.1:9000 s3 cp hello.txt s3://my-bucket/hello.txt

# ファイルをダウンロード
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

# バケットを作成
s3.create_bucket(Bucket="my-bucket")

# アップロード
s3.put_object(Bucket="my-bucket", Key="hello.txt", Body=b"Hello, devfs!")

# ダウンロード
resp = s3.get_object(Bucket="my-bucket", Key="hello.txt")
print(resp["Body"].read().decode())

# オブジェクト一覧
for obj in s3.list_objects_v2(Bucket="my-bucket").get("Contents", []):
    print(obj["Key"], obj["Size"])

# 削除
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

## 設定

設定の優先順位：**CLI フラグ > 環境変数 > 設定ファイル > デフォルト値**。

| パラメータ | CLI フラグ | 環境変数 | TOML キー | デフォルト値 |
|-----------|-----------|----------|----------|------------|
| ホスト | `--host` | `DEVFS_HOST` | `host` | `127.0.0.1` |
| ポート | `--port` | `DEVFS_PORT` | `port` | `9000` |
| データディレクトリ | `--data-dir` | `DEVFS_DATA_DIR` | `data_dir` | `./data` |
| Access Key | `--access-key` | `DEVFS_ACCESS_KEY` | `auth.access_key` | _(なし)_ |
| Secret Key | `--secret-key` | `DEVFS_SECRET_KEY` | `auth.secret_key` | _(なし)_ |
| Web ユーザー名 | `--web-user` | `DEVFS_WEB_USER` | `auth.web.user` | _(なし)_ |
| Web パスワード | `--web-password` | `DEVFS_WEB_PASSWORD` | `auth.web.password` | _(なし)_ |
| 設定ファイル | `--config` | — | — | `devfs.toml` |

`devfs.toml` の例：

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

## サポートされている操作

### バケット操作

| 操作 | メソッド | パス |
|------|---------|------|
| ListBuckets | `GET /` | すべてのバケットを一覧表示 |
| CreateBucket | `PUT /{bucket}` | バケットを作成 |
| DeleteBucket | `DELETE /{bucket}` | 空のバケットを削除 |
| HeadBucket | `HEAD /{bucket}` | バケットの存在を確認 |

### オブジェクト操作

| 操作 | メソッド | パス |
|------|---------|------|
| ListObjectsV2 | `GET /{bucket}?list-type=2` | バケット内のオブジェクトを一覧表示 |
| PutObject | `PUT /{bucket}/{key}` | オブジェクトをアップロード |
| GetObject | `GET /{bucket}/{key}` | オブジェクトをダウンロード |
| DeleteObject | `DELETE /{bucket}/{key}` | オブジェクトを削除 |
| HeadObject | `HEAD /{bucket}/{key}` | オブジェクトのメタデータを取得 |

### ListObjectsV2 クエリパラメータ

| パラメータ | 型 | デフォルト値 | 説明 |
|-----------|-----|------------|------|
| `prefix` | string | _(なし)_ | キープレフィックスでオブジェクトをフィルタリング |
| `delimiter` | string | _(なし)_ | デリミタでキーをグループ化（例：`/`） |
| `max-keys` | u32 | `1000` | 返すキーの最大数 |
| `start-after` | string | _(なし)_ | このキー以降から一覧表示を開始 |
| `continuation-token` | string | _(なし)_ | 前回のレスポンスから継続 |

## 認証

認証は**オプション**であり、どの認証情報が設定されているかによって動作が変わります：

| auth keys | auth.web | 動作 |
|-----------|----------|------|
| 空 | 未設定 | 認証なし — すべての S3 リクエストを許可 |
| 設定済み | 未設定 | 単一管理者キー、SigV4 が必要、すべてのバケット |
| 空 | 設定済み | Web UI のみ、マネージド API キーでバケットごとの権限 |
| 設定済み | 設定済み | 管理者キー（フルアクセス）+ マネージドキーでバケットごとの権限 |

auth keys が設定されている場合、devfs は AWS Signature Version 4 (SigV4) ヘッダーを検証するため、標準の AWS SDK や CLI ツールがそのまま使えます。空文字列は未設定として扱われます。

バケットポリシー（`public_read`、`public_write`）を使用すると、認証が有効な場合でも特定のバケットへの未認証アクセスを許可できます。

## Web 管理 UI

`devfs.toml` の `[auth.web]`、CLI フラグ、または環境変数（`DEVFS_WEB_USER` / `DEVFS_WEB_PASSWORD`）でユーザー名とパスワードを設定して Web UI を有効にします。有効にすると以下のアドレスでアクセスできます：

```
http://host:port/_web/
```

### 機能

- **バケット管理** — バケットの作成と削除
- **オブジェクトブラウザ** — 各バケット内でのオブジェクトのアップロード、ダウンロード、削除
- **API キー管理** — access/secret キーペアの作成と取り消し
- **バケットごとの権限** — キーごとにバケット単位で `read` または `read_write` アクセスを割り当て

### 一般的なワークフロー

1. `http://127.0.0.1:9000/_web/` を開き、設定した認証情報でログイン
2. ダッシュボードからバケットを作成
3. バケットに入り、ファイルをアップロード
4. **API Keys** で API キーを作成
5. キーにバケットごとの権限を割り当て（例：`my-bucket` に `read_write`）
6. 生成された access/secret キーペアを任意の S3 クライアントで使用

セッションは Cookie ベースで TTL は 24 時間、メモリに保存されます（サーバー再起動時に失われます）。

## API キーと権限

Web UI が有効な場合、UI を通じて複数の API キーを作成・管理できます。各キーにはバケットごとの権限レベルを付与できます：

- **none** — アクセス不可（デフォルト）
- **read** — GetObject、HeadObject、ListObjectsV2
- **read_write** — 完全な読み取り + PutObject、DeleteObject

キーデータは `{data_dir}/.devfs/keys.json` に永続化されます。バケットポリシーは `{data_dir}/.devfs/bucket_policies.json` に保存されます。

## ストレージレイアウト

オブジェクトはファイルシステム上に直接保存されます。メタデータはオブジェクトデータの隣にある `.meta/` ディレクトリに保持されます：

```
data/
├── my-bucket/
│   ├── .meta/
│   │   └── hello.txt.json      # メタデータ（content-type、etag など）
│   ├── hello.txt               # オブジェクトデータ
│   └── docs/
│       ├── .meta/
│       │   └── readme.txt.json
│       └── readme.txt
└── other-bucket/
    └── ...
```

## ベンチマーク

実行コマンド：`CI=true cargo bench --features bench-internals`。

### ストリーミングアップロード

| サイズ | 時間 | スループット |
|--------|------|------------|
| 1 KB | 107.6 µs | 9.08 MiB/s |
| 1 MB | 1.58 ms | 634 MiB/s |
| 10 MB | 14.0 ms | 712 MiB/s |

### ストリーミングダウンロード

| サイズ | 時間 | スループット |
|--------|------|------------|
| 1 KB | 42.9 µs | 22.7 MiB/s |
| 1 MB | 80.8 µs | 12.1 GiB/s |
| 10 MB | 539 µs | 18.1 GiB/s |

### ListObjectsV2

| シナリオ | 100 | 1,000 | 10,000 |
|---------|-----|-------|--------|
| flat | 1.31 ms | 17.0 ms | 184 ms |
| prefix_pruning | — | 1.18 ms | 18.9 ms |
| delimiter | — | 1.20 ms | 11.6 ms |

### ETag 整合性

| サイズ | 時間 |
|--------|------|
| 1 KB | 211 µs |
| 1 MB | 2.85 ms |

## プロジェクト構成

| ファイル | 行数 | 説明 |
|---------|------|------|
| `storage.rs` | 1044 | ファイルシステムベースのオブジェクトストレージ |
| `web/api.rs` | 425 | Web 管理 REST API ハンドラ |
| `middleware.rs` | 323 | S3 リクエストの解析とルーティング |
| `config.rs` | 308 | CLI、環境変数、TOML 設定の読み込み |
| `auth.rs` | 266 | SigV4 署名検証 + マルチキー認証 |
| `keystore.rs` | 260 | API キーとバケットポリシーの永続化 |
| `error.rs` | 228 | S3 エラーコードと XML エラーレスポンス |
| `xml.rs` | 182 | S3 XML レスポンスのシリアライズ |
| `types.rs` | 135 | 共有型とデータ構造 |
| `dispatcher.rs` | 134 | ハンドラへの操作ディスパッチ |
| `session.rs` | 72 | インメモリセッション管理 |
| `web/mod.rs` | 52 | Web UI ルーターとセッションミドルウェア |
| `main.rs` | 47 | サーバーエントリポイント |
| `web/static_files.rs` | 37 | 組み込み静的ファイル配信 |
