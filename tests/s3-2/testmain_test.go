//go:build integration

package s3_2

import (
	"fmt"
	"os"
	"testing"
)

func TestMain(m *testing.M) {
	setup()
	checkConnectivity()
	fmt.Println("connected to devfs, running fixed bucket tests...")

	code := m.Run()
	os.Exit(code)
}
