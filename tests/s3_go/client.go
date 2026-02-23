//go:build integration

package s3_go

import (
	"context"
	"crypto/md5"
	"fmt"
	"io"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/aws/aws-sdk-go-v2/aws"
	"github.com/aws/aws-sdk-go-v2/config"
	"github.com/aws/aws-sdk-go-v2/credentials"
	"github.com/aws/aws-sdk-go-v2/service/s3"
)

var s3Client *s3.Client

func envOrDefault(key, defaultVal string) string {
	if v := os.Getenv(key); v != "" {
		return v
	}
	return defaultVal
}

func setup() {
	endpoint := envOrDefault("DEVFS_ENDPOINT", "http://127.0.0.1:8181")
	accessKey := envOrDefault("DEVFS_ACCESS_KEY", "admin-ak")
	secretKey := envOrDefault("DEVFS_SECRET_KEY", "admin-sk")
	// accessKey := envOrDefault("DEVFS_ACCESS_KEY", "LLXUQHXFS1J8BMMRKRLC")
	// secretKey := envOrDefault("DEVFS_SECRET_KEY", "Yge7t9HMEIbsXzj3SDedDhIox57EkvI4EJ11Hr6y")
	region := envOrDefault("DEVFS_REGION", "us-east-1")

	cfg, err := config.LoadDefaultConfig(context.TODO(),
		config.WithRegion(region),
		config.WithCredentialsProvider(
			credentials.NewStaticCredentialsProvider(accessKey, secretKey, ""),
		),
	)
	if err != nil {
		fmt.Fprintf(os.Stderr, "failed to load AWS config: %v\n", err)
		os.Exit(1)
	}

	s3Client = s3.NewFromConfig(cfg, func(o *s3.Options) {
		o.BaseEndpoint = aws.String(endpoint)
		o.UsePathStyle = true
	})
}

func checkConnectivity() {
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	_, err := s3Client.ListBuckets(ctx, &s3.ListBucketsInput{})
	if err != nil {
		fmt.Fprintf(os.Stderr, "cannot connect to devfs: %v\n", err)
		fmt.Fprintln(os.Stderr, "make sure devfs is running (cargo run)")
		os.Exit(1)
	}
}

func teardown() {
	ctx := context.Background()
	out, err := s3Client.ListBuckets(ctx, &s3.ListBucketsInput{})
	if err != nil {
		return
	}
	for _, b := range out.Buckets {
		if strings.HasPrefix(*b.Name, "gotest-") {
			deleteAllObjectsQuiet(ctx, *b.Name)
			s3Client.DeleteBucket(ctx, &s3.DeleteBucketInput{
				Bucket: b.Name,
			})
		}
	}
}

func testBucketName(suffix string) string {
	return fmt.Sprintf("gotest-%d-%s", time.Now().UnixNano(), suffix)
}

func createTestBucket(t *testing.T, suffix string) string {
	t.Helper()
	bucket := testBucketName(suffix)
	ctx := context.Background()

	_, err := s3Client.CreateBucket(ctx, &s3.CreateBucketInput{
		Bucket: aws.String(bucket),
	})
	if err != nil {
		t.Fatalf("failed to create test bucket %s: %v", bucket, err)
	}

	t.Cleanup(func() {
		deleteAllObjectsQuiet(ctx, bucket)
		s3Client.DeleteBucket(ctx, &s3.DeleteBucketInput{
			Bucket: aws.String(bucket),
		})
	})

	return bucket
}

func putTestObject(t *testing.T, bucket, key string, body []byte) *s3.PutObjectOutput {
	t.Helper()
	ctx := context.Background()
	out, err := s3Client.PutObject(ctx, &s3.PutObjectInput{
		Bucket: aws.String(bucket),
		Key:    aws.String(key),
		Body:   strings.NewReader(string(body)),
	})
	if err != nil {
		t.Fatalf("failed to put object %s/%s: %v", bucket, key, err)
	}
	return out
}

func getTestObject(t *testing.T, bucket, key string) []byte {
	t.Helper()
	ctx := context.Background()
	out, err := s3Client.GetObject(ctx, &s3.GetObjectInput{
		Bucket: aws.String(bucket),
		Key:    aws.String(key),
	})
	if err != nil {
		t.Fatalf("failed to get object %s/%s: %v", bucket, key, err)
	}
	defer out.Body.Close()
	data, err := io.ReadAll(out.Body)
	if err != nil {
		t.Fatalf("failed to read object body %s/%s: %v", bucket, key, err)
	}
	return data
}

func deleteAllObjectsQuiet(ctx context.Context, bucket string) {
	out, err := s3Client.ListObjectsV2(ctx, &s3.ListObjectsV2Input{
		Bucket: aws.String(bucket),
	})
	if err != nil {
		return
	}
	for _, obj := range out.Contents {
		s3Client.DeleteObject(ctx, &s3.DeleteObjectInput{
			Bucket: aws.String(bucket),
			Key:    obj.Key,
		})
	}
}

func computeMD5(data []byte) string {
	h := md5.Sum(data)
	return fmt.Sprintf("\"%x\"", h)
}
