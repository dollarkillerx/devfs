//go:build integration

package s3_go

import (
	"bytes"
	"context"
	"io"
	"net/http"
	"testing"
	"time"

	"github.com/aws/aws-sdk-go-v2/aws"
	"github.com/aws/aws-sdk-go-v2/service/s3"
)

func TestPresignedPutObject(t *testing.T) {
	bucket := createTestBucket(t, "presign-put")
	ctx := context.Background()
	body := []byte("presigned upload content")

	presignClient := s3.NewPresignClient(s3Client)

	presignReq, err := presignClient.PresignPutObject(ctx, &s3.PutObjectInput{
		Bucket: aws.String(bucket),
		Key:    aws.String("presigned.txt"),
	}, s3.WithPresignExpires(15*time.Minute))
	if err != nil {
		t.Fatalf("PresignPutObject failed: %v", err)
	}

	// Upload using the presigned URL with a raw HTTP request
	httpReq, err := http.NewRequestWithContext(ctx, http.MethodPut, presignReq.URL, bytes.NewReader(body))
	if err != nil {
		t.Fatalf("failed to create HTTP request: %v", err)
	}

	resp, err := http.DefaultClient.Do(httpReq)
	if err != nil {
		t.Fatalf("presigned PUT request failed: %v", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		respBody, _ := io.ReadAll(resp.Body)
		t.Fatalf("presigned PUT returned status %d: %s", resp.StatusCode, string(respBody))
	}

	// Verify the object was uploaded correctly using the normal S3 client
	got := getTestObject(t, bucket, "presigned.txt")
	if !bytes.Equal(got, body) {
		t.Fatalf("body mismatch: got %q, want %q", got, body)
	}
}

func TestPresignedGetObject(t *testing.T) {
	bucket := createTestBucket(t, "presign-get")
	ctx := context.Background()
	body := []byte("content for presigned get")

	// Upload using normal S3 client
	putTestObject(t, bucket, "get-me.txt", body)

	// Generate presigned GET URL
	presignClient := s3.NewPresignClient(s3Client)

	presignReq, err := presignClient.PresignGetObject(ctx, &s3.GetObjectInput{
		Bucket: aws.String(bucket),
		Key:    aws.String("get-me.txt"),
	}, s3.WithPresignExpires(15*time.Minute))
	if err != nil {
		t.Fatalf("PresignGetObject failed: %v", err)
	}

	// Download using the presigned URL with a raw HTTP request
	httpReq, err := http.NewRequestWithContext(ctx, http.MethodGet, presignReq.URL, nil)
	if err != nil {
		t.Fatalf("failed to create HTTP request: %v", err)
	}

	resp, err := http.DefaultClient.Do(httpReq)
	if err != nil {
		t.Fatalf("presigned GET request failed: %v", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		respBody, _ := io.ReadAll(resp.Body)
		t.Fatalf("presigned GET returned status %d: %s", resp.StatusCode, string(respBody))
	}

	got, err := io.ReadAll(resp.Body)
	if err != nil {
		t.Fatalf("failed to read response body: %v", err)
	}

	if !bytes.Equal(got, body) {
		t.Fatalf("body mismatch: got %q, want %q", got, body)
	}
}

func TestPresignedPutObject_LargerFile(t *testing.T) {
	bucket := createTestBucket(t, "presign-large")
	ctx := context.Background()

	// 1 MiB file
	body := make([]byte, 1024*1024)
	for i := range body {
		body[i] = byte(i % 256)
	}

	presignClient := s3.NewPresignClient(s3Client)

	presignReq, err := presignClient.PresignPutObject(ctx, &s3.PutObjectInput{
		Bucket: aws.String(bucket),
		Key:    aws.String("large-presigned.bin"),
	}, s3.WithPresignExpires(15*time.Minute))
	if err != nil {
		t.Fatalf("PresignPutObject failed: %v", err)
	}

	httpReq, err := http.NewRequestWithContext(ctx, http.MethodPut, presignReq.URL, bytes.NewReader(body))
	if err != nil {
		t.Fatalf("failed to create HTTP request: %v", err)
	}

	resp, err := http.DefaultClient.Do(httpReq)
	if err != nil {
		t.Fatalf("presigned PUT request failed: %v", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		respBody, _ := io.ReadAll(resp.Body)
		t.Fatalf("presigned PUT returned status %d: %s", resp.StatusCode, string(respBody))
	}

	got := getTestObject(t, bucket, "large-presigned.bin")
	if !bytes.Equal(got, body) {
		t.Fatal("body mismatch for 1 MiB presigned upload")
	}
}

func TestPresignedPutObject_NestedKey(t *testing.T) {
	bucket := createTestBucket(t, "presign-nested")
	ctx := context.Background()
	body := []byte("nested presigned content")
	key := "a/b/c/nested-presigned.txt"

	presignClient := s3.NewPresignClient(s3Client)

	presignReq, err := presignClient.PresignPutObject(ctx, &s3.PutObjectInput{
		Bucket: aws.String(bucket),
		Key:    aws.String(key),
	}, s3.WithPresignExpires(15*time.Minute))
	if err != nil {
		t.Fatalf("PresignPutObject failed: %v", err)
	}

	httpReq, err := http.NewRequestWithContext(ctx, http.MethodPut, presignReq.URL, bytes.NewReader(body))
	if err != nil {
		t.Fatalf("failed to create HTTP request: %v", err)
	}

	resp, err := http.DefaultClient.Do(httpReq)
	if err != nil {
		t.Fatalf("presigned PUT request failed: %v", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		respBody, _ := io.ReadAll(resp.Body)
		t.Fatalf("presigned PUT returned status %d: %s", resp.StatusCode, string(respBody))
	}

	got := getTestObject(t, bucket, key)
	if !bytes.Equal(got, body) {
		t.Fatalf("body mismatch: got %q, want %q", got, body)
	}
}
