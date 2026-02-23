IMG_REGISTRY ?= dollarkiller
IMG_TAG      ?= latest

.PHONY: build run clean bench img-build compose-up compose-down test-go

build:
	cargo build --release

run:
	cargo run --release

clean:
	cargo clean

bench:
	CI=true cargo bench --features bench-internals

img-build:
	docker build -t $(IMG_REGISTRY)/devfs:$(IMG_TAG) .

compose-up:
	docker compose up -d

compose-down:
	docker compose down

test-go:
	cd tests/s3_go && go test -v -tags integration -timeout 60s ./...

test-go2:
	cd tests/s3-2 && go test -tags integration -v -run TestFixedBucket ./...
