FROM rust:1.77-slim as builder

WORKDIR /usr/src/app

# Cache dependencies by building them first
COPY Cargo.toml Cargo.lock* ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release 2>/dev/null || true
RUN rm -rf src

# Build the actual application
COPY . .
RUN cargo build --package piarch-a-token-rs --release

# Runtime stage
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /usr/src/app/target/release/piarch-a-token-rs .
COPY keys/ ./keys/

EXPOSE 10000

CMD ["./piarch-a-token-rs"]
