# ==========================================================================
# Cortex — Dockerfile
# Multi-stage build: compiles in a full Rust image, runs in a slim image.
# All dependencies are self-contained — nothing touches the host OS.
#
# Build:  docker build -t cortex .
# Run:    docker run --rm -v /path/to/project:/project cortex index /project
# MCP:    docker run --rm -i -v /path/to/project:/project cortex serve
# ==========================================================================

# ---------- Stage 1: Build ----------
FROM rust:bookworm AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev protobuf-compiler && rm -rf /var/lib/apt/lists/*

WORKDIR /usr/src/cortex

# Cache dependencies by building manifests first
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release --all-features 2>/dev/null || true

# Copy full source and build the real binary
COPY . .
RUN touch src/main.rs && cargo build --release --all-features

# ---------- Stage 2: Runtime ----------
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates libssl3 && \
    rm -rf /var/lib/apt/lists/*

RUN useradd --create-home cortex
USER cortex

WORKDIR /project

COPY --from=builder /usr/src/cortex/target/release/cortex /usr/local/bin/cortex

VOLUME ["/home/cortex/.cortex"]

ENTRYPOINT ["cortex"]
CMD ["--help"]
