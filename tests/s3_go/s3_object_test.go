//go:build integration

package s3_go

import (
	"context"
	"strings"
	"testing"

	"github.com/aws/aws-sdk-go-v2/aws"
	"github.com/aws/aws-sdk-go-v2/service/s3"
)

func TestGetObject_NotFound(t *testing.T) {
	bucket := createTestBucket(t, "get-notfound")

	_, err := s3Client.GetObject(context.Background(), &s3.GetObjectInput{
		Bucket: aws.String(bucket),
		Key:    aws.String("does-not-exist.txt"),
	})
	if err == nil {
		t.Fatal("expected error for non-existent key, got nil")
	}
	if !strings.Contains(err.Error(), "NoSuchKey") {
		t.Fatalf("expected NoSuchKey error, got: %v", err)
	}
}

func TestHeadObject_ReturnsMetadata(t *testing.T) {
	bucket := createTestBucket(t, "head-meta")
	ctx := context.Background()
	body := []byte("head object test content")

	_, err := s3Client.PutObject(ctx, &s3.PutObjectInput{
		Bucket:      aws.String(bucket),
		Key:         aws.String("head.txt"),
		Body:        strings.NewReader(string(body)),
		ContentType: aws.String("text/plain"),
	})
	if err != nil {
		t.Fatalf("PutObject failed: %v", err)
	}

	head, err := s3Client.HeadObject(ctx, &s3.HeadObjectInput{
		Bucket: aws.String(bucket),
		Key:    aws.String("head.txt"),
	})
	if err != nil {
		t.Fatalf("HeadObject failed: %v", err)
	}

	if head.ContentType == nil || *head.ContentType != "text/plain" {
		t.Errorf("content-type: got %v, want text/plain", head.ContentType)
	}
	if head.ContentLength == nil || *head.ContentLength != int64(len(body)) {
		t.Errorf("content-length: got %v, want %d", head.ContentLength, len(body))
	}
	if head.ETag == nil || *head.ETag == "" {
		t.Error("ETag is empty or nil")
	}
	expectedETag := computeMD5(body)
	if *head.ETag != expectedETag {
		t.Errorf("ETag: got %q, want %q", *head.ETag, expectedETag)
	}
}

func TestDeleteObject_Existing(t *testing.T) {
	bucket := createTestBucket(t, "del-existing")
	ctx := context.Background()

	putTestObject(t, bucket, "to-delete.txt", []byte("delete me"))

	_, err := s3Client.DeleteObject(ctx, &s3.DeleteObjectInput{
		Bucket: aws.String(bucket),
		Key:    aws.String("to-delete.txt"),
	})
	if err != nil {
		t.Fatalf("DeleteObject failed: %v", err)
	}

	_, err = s3Client.GetObject(ctx, &s3.GetObjectInput{
		Bucket: aws.String(bucket),
		Key:    aws.String("to-delete.txt"),
	})
	if err == nil {
		t.Fatal("expected error after deleting object, got nil")
	}
}

func TestDeleteObject_Idempotent(t *testing.T) {
	bucket := createTestBucket(t, "del-idempotent")

	_, err := s3Client.DeleteObject(context.Background(), &s3.DeleteObjectInput{
		Bucket: aws.String(bucket),
		Key:    aws.String("never-existed.txt"),
	})
	if err != nil {
		t.Fatalf("DeleteObject on non-existent key should not error, got: %v", err)
	}
}

func TestListObjectsV2_Basic(t *testing.T) {
	bucket := createTestBucket(t, "list-basic")

	keys := []string{"alpha.txt", "beta.txt", "gamma.txt"}
	for _, k := range keys {
		putTestObject(t, bucket, k, []byte("content-"+k))
	}

	out, err := s3Client.ListObjectsV2(context.Background(), &s3.ListObjectsV2Input{
		Bucket: aws.String(bucket),
	})
	if err != nil {
		t.Fatalf("ListObjectsV2 failed: %v", err)
	}

	found := map[string]bool{}
	for _, obj := range out.Contents {
		found[*obj.Key] = true
	}

	for _, k := range keys {
		if !found[k] {
			t.Errorf("key %q not found in list response", k)
		}
	}
}

func TestListObjectsV2_Prefix(t *testing.T) {
	bucket := createTestBucket(t, "list-prefix")

	putTestObject(t, bucket, "logs/app.log", []byte("log1"))
	putTestObject(t, bucket, "logs/error.log", []byte("log2"))
	putTestObject(t, bucket, "data/file.csv", []byte("csv"))

	out, err := s3Client.ListObjectsV2(context.Background(), &s3.ListObjectsV2Input{
		Bucket: aws.String(bucket),
		Prefix: aws.String("logs/"),
	})
	if err != nil {
		t.Fatalf("ListObjectsV2 failed: %v", err)
	}

	if len(out.Contents) != 2 {
		t.Fatalf("expected 2 objects with prefix logs/, got %d", len(out.Contents))
	}

	for _, obj := range out.Contents {
		if !strings.HasPrefix(*obj.Key, "logs/") {
			t.Errorf("unexpected key %q in prefix-filtered list", *obj.Key)
		}
	}
}
