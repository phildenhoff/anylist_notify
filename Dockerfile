# Build stage
FROM rust:1.83-slim AS builder

# Install build dependencies (including git for fetching git dependencies)
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    git \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy manifests first for better layer caching
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src

# Build the application (will fetch anylist_rs from git)
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the built binary from builder
COPY --from=builder /app/target/release/anylist_notify /usr/local/bin/anylist_notify

# Create directory for SQLite database
RUN mkdir -p /data

# Set environment variable for database path
ENV DATABASE_PATH=/data/anylist.db

# Run as non-root user
RUN useradd -m -u 1000 appuser && \
    chown -R appuser:appuser /app /data
USER appuser

CMD ["anylist_notify"]
