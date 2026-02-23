//go:build integration

package s3_go

import (
	"bytes"
	"context"
	"crypto/rand"
	"strings"
	"testing"

	"github.com/aws/aws-sdk-go-v2/aws"
	"github.com/aws/aws-sdk-go-v2/service/s3"
)

func TestPutObject_BasicText(t *testing.T) {
	bucket := createTestBucket(t, "put-basic")
	body := []byte("Hello, devfs!")

	putTestObject(t, bucket, "hello.txt", body)

	got := getTestObject(t, bucket, "hello.txt")
	if !bytes.Equal(got, body) {
		t.Fatalf("body mismatch: got %q, want %q", got, body)
	}
}

func TestPutObject_CustomContentType(t *testing.T) {
	bucket := createTestBucket(t, "put-ct")
	ctx := context.Background()

	_, err := s3Client.PutObject(ctx, &s3.PutObjectInput{
		Bucket:      aws.String(bucket),
		Key:         aws.String("data.json"),
		Body:        strings.NewReader(`{"key":"value"}`),
		ContentType: aws.String("application/json"),
	})
	if err != nil {
		t.Fatalf("PutObject failed: %v", err)
	}

	head, err := s3Client.HeadObject(ctx, &s3.HeadObjectInput{
		Bucket: aws.String(bucket),
		Key:    aws.String("data.json"),
	})
	if err != nil {
		t.Fatalf("HeadObject failed: %v", err)
	}

	if head.ContentType == nil || *head.ContentType != "application/json" {
		t.Fatalf("content-type mismatch: got %v, want application/json", head.ContentType)
	}
}

func TestPutObject_CustomMetadata(t *testing.T) {
	bucket := createTestBucket(t, "put-meta")
	ctx := context.Background()

	meta := map[string]string{
		"author":  "tester",
		"version": "42",
	}

	_, err := s3Client.PutObject(ctx, &s3.PutObjectInput{
		Bucket:   aws.String(bucket),
		Key:      aws.String("meta.txt"),
		Body:     strings.NewReader("metadata test"),
		Metadata: meta,
	})
	if err != nil {
		t.Fatalf("PutObject failed: %v", err)
	}

	head, err := s3Client.HeadObject(ctx, &s3.HeadObjectInput{
		Bucket: aws.String(bucket),
		Key:    aws.String("meta.txt"),
	})
	if err != nil {
		t.Fatalf("HeadObject failed: %v", err)
	}

	for k, want := range meta {
		got, ok := head.Metadata[k]
		if !ok {
			t.Errorf("metadata key %q not found", k)
			continue
		}
		if got != want {
			t.Errorf("metadata[%q] = %q, want %q", k, got, want)
		}
	}
}

func TestPutObject_EmptyBody(t *testing.T) {
	bucket := createTestBucket(t, "put-empty")
	ctx := context.Background()

	putTestObject(t, bucket, "empty.txt", []byte{})

	head, err := s3Client.HeadObject(ctx, &s3.HeadObjectInput{
		Bucket: aws.String(bucket),
		Key:    aws.String("empty.txt"),
	})
	if err != nil {
		t.Fatalf("HeadObject failed: %v", err)
	}

	if head.ContentLength == nil || *head.ContentLength != 0 {
		t.Fatalf("content-length mismatch: got %v, want 0", head.ContentLength)
	}

	got := getTestObject(t, bucket, "empty.txt")
	if len(got) != 0 {
		t.Fatalf("expected empty body, got %d bytes", len(got))
	}
}

func TestPutObject_LargerObject(t *testing.T) {
	bucket := createTestBucket(t, "put-large")

	size := 5 * 1024 * 1024 // 5 MiB
	data := make([]byte, size)
	if _, err := rand.Read(data); err != nil {
		t.Fatalf("failed to generate random data: %v", err)
	}

	out := putTestObject(t, bucket, "large.bin", data)

	got := getTestObject(t, bucket, "large.bin")
	if !bytes.Equal(got, data) {
		t.Fatal("body mismatch for 5 MiB object")
	}

	expectedETag := computeMD5(data)
	if out.ETag != nil && *out.ETag != expectedETag {
		t.Fatalf("ETag mismatch: got %q, want %q", *out.ETag, expectedETag)
	}
}

func TestPutObject_VerifyETag(t *testing.T) {
	bucket := createTestBucket(t, "put-etag")
	body := []byte("etag verification content")

	out := putTestObject(t, bucket, "etag.txt", body)

	expectedETag := computeMD5(body)
	if out.ETag == nil {
		t.Fatal("ETag is nil")
	}
	if *out.ETag != expectedETag {
		t.Fatalf("ETag mismatch: got %q, want %q", *out.ETag, expectedETag)
	}
}

func TestPutObject_OverwriteExisting(t *testing.T) {
	bucket := createTestBucket(t, "put-overwrite")

	putTestObject(t, bucket, "file.txt", []byte("first version"))
	putTestObject(t, bucket, "file.txt", []byte("second version"))

	got := getTestObject(t, bucket, "file.txt")
	if string(got) != "second version" {
		t.Fatalf("expected overwritten content, got %q", got)
	}
}

func TestPutObject_NonExistentBucket(t *testing.T) {
	ctx := context.Background()
	_, err := s3Client.PutObject(ctx, &s3.PutObjectInput{
		Bucket: aws.String("gotest-nonexistent-bucket-xyz"),
		Key:    aws.String("file.txt"),
		Body:   strings.NewReader("data"),
	})
	if err == nil {
		t.Fatal("expected error when putting to non-existent bucket, got nil")
	}
	if !strings.Contains(err.Error(), "NoSuchBucket") {
		t.Fatalf("expected NoSuchBucket error, got: %v", err)
	}
}

func TestPutObject_NestedKey(t *testing.T) {
	bucket := createTestBucket(t, "put-nested")
	ctx := context.Background()
	key := "a/b/c/deep.txt"

	putTestObject(t, bucket, key, []byte("nested content"))

	got := getTestObject(t, bucket, key)
	if string(got) != "nested content" {
		t.Fatalf("body mismatch: got %q", got)
	}

	list, err := s3Client.ListObjectsV2(ctx, &s3.ListObjectsV2Input{
		Bucket: aws.String(bucket),
		Prefix: aws.String("a/b/"),
	})
	if err != nil {
		t.Fatalf("ListObjectsV2 failed: %v", err)
	}

	found := false
	for _, obj := range list.Contents {
		if *obj.Key == key {
			found = true
			break
		}
	}
	if !found {
		t.Fatalf("nested key %q not found in list with prefix", key)
	}
}

func TestPutObject_SpecialCharactersInKey(t *testing.T) {
	bucket := createTestBucket(t, "put-special")
	key := "file with spaces (1).txt"

	putTestObject(t, bucket, key, []byte("special chars"))

	got := getTestObject(t, bucket, key)
	if string(got) != "special chars" {
		t.Fatalf("body mismatch: got %q", got)
	}
}

func TestPutObject_BinaryContent(t *testing.T) {
	bucket := createTestBucket(t, "put-binary")

	data := make([]byte, 256)
	for i := range data {
		data[i] = byte(i)
	}

	putTestObject(t, bucket, "binary.dat", data)

	got := getTestObject(t, bucket, "binary.dat")
	if !bytes.Equal(got, data) {
		t.Fatal("binary content mismatch")
	}
}
