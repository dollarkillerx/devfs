//go:build integration

package s3_go

import (
	"context"
	"strings"
	"testing"

	"github.com/aws/aws-sdk-go-v2/aws"
	"github.com/aws/aws-sdk-go-v2/service/s3"
)

func TestCreateBucket_Basic(t *testing.T) {
	bucket := createTestBucket(t, "create-basic")

	_, err := s3Client.HeadBucket(context.Background(), &s3.HeadBucketInput{
		Bucket: aws.String(bucket),
	})
	if err != nil {
		t.Fatalf("HeadBucket failed for newly created bucket: %v", err)
	}
}

func TestCreateBucket_AlreadyExists(t *testing.T) {
	bucket := createTestBucket(t, "create-dup")

	_, err := s3Client.CreateBucket(context.Background(), &s3.CreateBucketInput{
		Bucket: aws.String(bucket),
	})
	if err == nil {
		t.Fatal("expected error when creating duplicate bucket, got nil")
	}
	if !strings.Contains(err.Error(), "BucketAlreadyExists") {
		t.Fatalf("expected BucketAlreadyExists error, got: %v", err)
	}
}

func TestDeleteBucket_Empty(t *testing.T) {
	bucket := testBucketName("delete-empty")
	ctx := context.Background()

	_, err := s3Client.CreateBucket(ctx, &s3.CreateBucketInput{
		Bucket: aws.String(bucket),
	})
	if err != nil {
		t.Fatalf("failed to create bucket: %v", err)
	}

	_, err = s3Client.DeleteBucket(ctx, &s3.DeleteBucketInput{
		Bucket: aws.String(bucket),
	})
	if err != nil {
		t.Fatalf("failed to delete empty bucket: %v", err)
	}

	_, err = s3Client.HeadBucket(ctx, &s3.HeadBucketInput{
		Bucket: aws.String(bucket),
	})
	if err == nil {
		t.Fatal("expected error after deleting bucket, got nil")
	}
}

func TestDeleteBucket_NotEmpty(t *testing.T) {
	bucket := createTestBucket(t, "delete-notempty")
	putTestObject(t, bucket, "file.txt", []byte("data"))

	_, err := s3Client.DeleteBucket(context.Background(), &s3.DeleteBucketInput{
		Bucket: aws.String(bucket),
	})
	if err == nil {
		t.Fatal("expected error when deleting non-empty bucket, got nil")
	}
	if !strings.Contains(err.Error(), "BucketNotEmpty") {
		t.Fatalf("expected BucketNotEmpty error, got: %v", err)
	}
}

func TestListBuckets(t *testing.T) {
	b1 := createTestBucket(t, "list-1")
	b2 := createTestBucket(t, "list-2")
	b3 := createTestBucket(t, "list-3")

	out, err := s3Client.ListBuckets(context.Background(), &s3.ListBucketsInput{})
	if err != nil {
		t.Fatalf("ListBuckets failed: %v", err)
	}

	found := map[string]bool{}
	for _, b := range out.Buckets {
		found[*b.Name] = true
	}

	for _, name := range []string{b1, b2, b3} {
		if !found[name] {
			t.Errorf("bucket %s not found in ListBuckets response", name)
		}
	}
}
