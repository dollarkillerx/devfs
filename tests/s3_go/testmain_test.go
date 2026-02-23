//go:build integration

package s3_go

import (
	"fmt"
	"os"
	"testing"
)

func TestMain(m *testing.M) {
	setup()
	checkConnectivity()
	fmt.Println("connected to devfs, running tests...")

	code := m.Run()

	teardown()
	os.Exit(code)
}
