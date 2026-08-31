FROM rust:1.97-slim AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release && ls -lh target/release/memogram-rs && test $(stat -c%s target/release/memogram-rs) -gt 1000000

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/memogram-rs /usr/local/bin/
RUN useradd -u 65532 -m bot
USER 65532
ENTRYPOINT ["memogram-rs"]
