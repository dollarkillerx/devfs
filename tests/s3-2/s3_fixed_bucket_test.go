//go:build integration

package s3_2

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"strings"
	"testing"
	"time"

	"github.com/aws/aws-sdk-go-v2/aws"
	"github.com/aws/aws-sdk-go-v2/service/s3"
)

const fixedBucket = "mkkb"

func fixedTestKey(suffix string) string {
	return fmt.Sprintf("fixedtest-%d-%s", time.Now().UnixNano(), suffix)
}

func cleanupObject(t *testing.T, bucket, key string) {
	t.Helper()
	t.Cleanup(func() {
		ctx := context.Background()
		s3Client.DeleteObject(ctx, &s3.DeleteObjectInput{
			Bucket: aws.String(bucket),
			Key:    aws.String(key),
		})
	})
}

func TestFixedBucket_Exists(t *testing.T) {
	ctx := context.Background()
	_, err := s3Client.HeadBucket(ctx, &s3.HeadBucketInput{
		Bucket: aws.String(fixedBucket),
	})
	if err != nil {
		t.Fatalf("expected bucket %s to exist: %v", fixedBucket, err)
	}
}

func TestFixedBucket_PutAndGetObject(t *testing.T) {
	key := fixedTestKey("putget")
	cleanupObject(t, fixedBucket, key)

	body := []byte("hello fixed bucket")
	ctx := context.Background()

	_, err := s3Client.PutObject(ctx, &s3.PutObjectInput{
		Bucket: aws.String(fixedBucket),
		Key:    aws.String(key),
		Body:   bytes.NewReader(body),
	})
	if err != nil {
		t.Fatalf("PutObject failed: %v", err)
	}

	out, err := s3Client.GetObject(ctx, &s3.GetObjectInput{
		Bucket: aws.String(fixedBucket),
		Key:    aws.String(key),
	})
	if err != nil {
		t.Fatalf("GetObject failed: %v", err)
	}
	defer out.Body.Close()

	data, err := io.ReadAll(out.Body)
	if err != nil {
		t.Fatalf("failed to read body: %v", err)
	}
	if !bytes.Equal(data, body) {
		t.Fatalf("body mismatch: got %q, want %q", data, body)
	}
}

func TestFixedBucket_HeadObject(t *testing.T) {
	key := fixedTestKey("head")
	cleanupObject(t, fixedBucket, key)

	body := []byte("head-test-content")
	ctx := context.Background()

	_, err := s3Client.PutObject(ctx, &s3.PutObjectInput{
		Bucket:      aws.String(fixedBucket),
		Key:         aws.String(key),
		Body:        bytes.NewReader(body),
		ContentType: aws.String("text/plain"),
	})
	if err != nil {
		t.Fatalf("PutObject failed: %v", err)
	}

	head, err := s3Client.HeadObject(ctx, &s3.HeadObjectInput{
		Bucket: aws.String(fixedBucket),
		Key:    aws.String(key),
	})
	if err != nil {
		t.Fatalf("HeadObject failed: %v", err)
	}

	if head.ContentLength == nil || *head.ContentLength != int64(len(body)) {
		t.Fatalf("ContentLength mismatch: got %v, want %d", head.ContentLength, len(body))
	}
	if head.ETag == nil || *head.ETag == "" {
		t.Fatal("expected non-empty ETag")
	}
}

func TestFixedBucket_ListObjects(t *testing.T) {
	key := fixedTestKey("list")
	cleanupObject(t, fixedBucket, key)

	ctx := context.Background()
	_, err := s3Client.PutObject(ctx, &s3.PutObjectInput{
		Bucket: aws.String(fixedBucket),
		Key:    aws.String(key),
		Body:   strings.NewReader("list-content"),
	})
	if err != nil {
		t.Fatalf("PutObject failed: %v", err)
	}

	out, err := s3Client.ListObjectsV2(ctx, &s3.ListObjectsV2Input{
		Bucket: aws.String(fixedBucket),
	})
	if err != nil {
		t.Fatalf("ListObjectsV2 failed: %v", err)
	}

	found := false
	for _, obj := range out.Contents {
		if *obj.Key == key {
			found = true
			break
		}
	}
	if !found {
		t.Fatalf("expected key %s in listing", key)
	}
}

func TestFixedBucket_ListObjectsPrefix(t *testing.T) {
	prefix := fixedTestKey("pfx") + "/"
	keys := []string{prefix + "a.txt", prefix + "b.txt"}
	for _, k := range keys {
		cleanupObject(t, fixedBucket, k)
	}

	ctx := context.Background()
	for _, k := range keys {
		_, err := s3Client.PutObject(ctx, &s3.PutObjectInput{
			Bucket: aws.String(fixedBucket),
			Key:    aws.String(k),
			Body:   strings.NewReader("pfx-content"),
		})
		if err != nil {
			t.Fatalf("PutObject %s failed: %v", k, err)
		}
	}

	out, err := s3Client.ListObjectsV2(ctx, &s3.ListObjectsV2Input{
		Bucket: aws.String(fixedBucket),
		Prefix: aws.String(prefix),
	})
	if err != nil {
		t.Fatalf("ListObjectsV2 with prefix failed: %v", err)
	}

	if len(out.Contents) < 2 {
		t.Fatalf("expected at least 2 objects with prefix %s, got %d", prefix, len(out.Contents))
	}
	for _, obj := range out.Contents {
		if !strings.HasPrefix(*obj.Key, prefix) {
			t.Fatalf("object %s does not have prefix %s", *obj.Key, prefix)
		}
	}
}

func TestFixedBucket_DeleteObject(t *testing.T) {
	key := fixedTestKey("del")
	ctx := context.Background()

	_, err := s3Client.PutObject(ctx, &s3.PutObjectInput{
		Bucket: aws.String(fixedBucket),
		Key:    aws.String(key),
		Body:   strings.NewReader("delete-me"),
	})
	if err != nil {
		t.Fatalf("PutObject failed: %v", err)
	}

	_, err = s3Client.DeleteObject(ctx, &s3.DeleteObjectInput{
		Bucket: aws.String(fixedBucket),
		Key:    aws.String(key),
	})
	if err != nil {
		t.Fatalf("DeleteObject failed: %v", err)
	}

	_, err = s3Client.HeadObject(ctx, &s3.HeadObjectInput{
		Bucket: aws.String(fixedBucket),
		Key:    aws.String(key),
	})
	if err == nil {
		t.Fatal("expected error after deleting object, but HeadObject succeeded")
	}
}

func TestFixedBucket_OverwriteObject(t *testing.T) {
	key := fixedTestKey("overwrite")
	cleanupObject(t, fixedBucket, key)

	ctx := context.Background()

	_, err := s3Client.PutObject(ctx, &s3.PutObjectInput{
		Bucket: aws.String(fixedBucket),
		Key:    aws.String(key),
		Body:   strings.NewReader("version1"),
	})
	if err != nil {
		t.Fatalf("PutObject v1 failed: %v", err)
	}

	_, err = s3Client.PutObject(ctx, &s3.PutObjectInput{
		Bucket: aws.String(fixedBucket),
		Key:    aws.String(key),
		Body:   strings.NewReader("version2"),
	})
	if err != nil {
		t.Fatalf("PutObject v2 failed: %v", err)
	}

	out, err := s3Client.GetObject(ctx, &s3.GetObjectInput{
		Bucket: aws.String(fixedBucket),
		Key:    aws.String(key),
	})
	if err != nil {
		t.Fatalf("GetObject failed: %v", err)
	}
	defer out.Body.Close()

	data, err := io.ReadAll(out.Body)
	if err != nil {
		t.Fatalf("failed to read body: %v", err)
	}
	if string(data) != "version2" {
		t.Fatalf("expected 'version2', got %q", data)
	}
}

func TestFixedBucket_NestedKey(t *testing.T) {
	key := fixedTestKey("nested") + "/deep/path/file.txt"
	cleanupObject(t, fixedBucket, key)

	body := []byte("nested-content")
	ctx := context.Background()

	_, err := s3Client.PutObject(ctx, &s3.PutObjectInput{
		Bucket: aws.String(fixedBucket),
		Key:    aws.String(key),
		Body:   bytes.NewReader(body),
	})
	if err != nil {
		t.Fatalf("PutObject failed: %v", err)
	}

	out, err := s3Client.GetObject(ctx, &s3.GetObjectInput{
		Bucket: aws.String(fixedBucket),
		Key:    aws.String(key),
	})
	if err != nil {
		t.Fatalf("GetObject failed: %v", err)
	}
	defer out.Body.Close()

	data, err := io.ReadAll(out.Body)
	if err != nil {
		t.Fatalf("failed to read body: %v", err)
	}
	if !bytes.Equal(data, body) {
		t.Fatalf("body mismatch: got %q, want %q", data, body)
	}
}
