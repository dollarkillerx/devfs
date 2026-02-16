FROM rust:1.93-slim AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates tzdata \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src/ src/
COPY web_ui/ web_ui/
COPY benches/ benches/

RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates tzdata \
 && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=builder /app/target/release/devfs .
EXPOSE 9000
VOLUME ["/data"]
CMD ["./devfs", "--host", "0.0.0.0", "--data-dir", "/data"]
