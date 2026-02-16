IMG_REGISTRY ?= dollarkiller
IMG_TAG      ?= latest

.PHONY: build run clean bench img-build compose-up compose-down

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
