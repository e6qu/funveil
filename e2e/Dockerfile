# E2E Test Environment for Funveil
FROM rust:1.75-slim-bookworm

# Install dependencies
RUN apt-get update && apt-get install -y \
    git \
    make \
    curl \
    file \
    && rm -rf /var/lib/apt/lists/*

# Install cargo tools for testing
RUN cargo install cargo-audit cargo-deny --locked

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
