FROM rust:1.88-slim AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
COPY Cargo.toml Cargo.lock* ./
RUN cargo update -p takecell --precise 0.1.0 2>&1 || cargo update -p takecell --precise 0.1.1 2>&1 || true
RUN mkdir src && echo "fn main(){}" > src/main.rs && cargo build --release || true
COPY src ./src
RUN cargo update -p takecell --precise 0.1.0 2>&1 || cargo update -p takecell --precise 0.1.1 2>&1 || true
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates libssl3 && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/memogram-rs /usr/local/bin/
RUN useradd -u 65532 -m bot
USER 65532
ENTRYPOINT ["memogram-rs"]