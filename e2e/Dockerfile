# E2E Test Environment for Funveil
# Rust 1.94.0 (latest stable as of 2026-03-10)
# Debian Bookworm (12) slim
FROM rust:1.94.0-slim-bookworm

# Install dependencies with pinned versions where possible
# git: 2.39.5
# make: 4.3
# curl: 7.88.1
# file: 5.44
# build-essential: for compiling tree-sitter grammars (C code)
RUN apt-get update && apt-get install -y \
    git=1:2.39.5-* \
    make=4.3-* \
    curl=7.88.1-* \
    file=1:5.44-* \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# Create test workspace
WORKDIR /workspace

# Copy the project
COPY . /workspace/funveil/

# Build the project
WORKDIR /workspace/funveil
RUN cargo build --release

# Install the binary to PATH
RUN cp target/release/fv /usr/local/bin/fv

# Create test directory structure
RUN mkdir -p /workspace/test-project

WORKDIR /workspace/test-project

# Default command runs e2e tests
CMD ["/workspace/funveil/e2e/run-e2e.sh"]
