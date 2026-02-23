//go:build integration

package s3_2

import (
	"context"
	"fmt"
	"os"
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
	accessKey := envOrDefault("DEVFS_ACCESS_KEY", "LLXUQHXFS1J8BMMRKRLC")
	secretKey := envOrDefault("DEVFS_SECRET_KEY", "Yge7t9HMEIbsXzj3SDedDhIox57EkvI4EJ11Hr6y")
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
