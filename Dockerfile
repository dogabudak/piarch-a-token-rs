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
COPY src/*.pem ./src/

EXPOSE 10000

# Map Render's PORT env var to Rocket's config
CMD ROCKET_PORT=${PORT:-10000} ROCKET_ADDRESS=0.0.0.0 ./piarch-a-token-rs
