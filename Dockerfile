# syntax=docker/dockerfile:1

# --- Builder stage ---
FROM rust:1-bookworm AS builder

ARG NODE_VERSION=22
ARG GO_VERSION=1.23.6
ARG TARGETARCH

# System deps: libclang for bindgen, cmake for Z3 build, libz3-dev for headers/libs
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    libclang-dev \
    cmake \
    libz3-dev \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js (needed by build.rs to bundle shatter-ts)
RUN curl -fsSL https://deb.nodesource.com/setup_${NODE_VERSION}.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

# Install Go (needed by build.rs to compile shatter-go)
RUN GOARCH=$(case "${TARGETARCH}" in arm64) echo "arm64" ;; *) echo "amd64" ;; esac) \
    && curl -fsSL "https://go.dev/dl/go${GO_VERSION}.linux-${GOARCH}.tar.gz" | tar -C /usr/local -xz
ENV PATH="/usr/local/go/bin:${PATH}"

WORKDIR /build

# Copy dependency manifests first for layer caching
COPY Cargo.toml Cargo.lock ./
COPY shatter-core/Cargo.toml shatter-core/Cargo.toml
COPY shatter-cli/Cargo.toml shatter-cli/Cargo.toml
COPY shatter-ts/package.json shatter-ts/package-lock.json* shatter-ts/
COPY shatter-go/go.mod shatter-go/go.sum* shatter-go/

# Copy full source
COPY shatter-core/ shatter-core/
COPY shatter-cli/ shatter-cli/
COPY shatter-ts/ shatter-ts/
COPY shatter-go/ shatter-go/
COPY shatter-rust/ shatter-rust/

# Build Z3 from source instead of using the prebuilt gh-release binary.
# The prebuilt binary requires glibc 2.39 (Ubuntu 24.04), but building from
# source links against the builder's glibc (2.36/Bookworm), which matches
# node:22-slim at runtime — saving ~130MB in image size.
RUN sed -i 's/features = \["gh-release"\]/features = []/' shatter-core/Cargo.toml

# Build the main CLI (build.rs bundles TS and compiles Go frontend)
RUN cargo build --release -p shatter-cli

# Build shatter-rust frontend (separate from workspace)
RUN cargo build --release --manifest-path shatter-rust/Cargo.toml

# Collect any dynamically-linked Z3 libs for the runtime stage
RUN mkdir -p /build/libs \
    && (ldd target/release/shatter 2>/dev/null | grep -oP '/\S+libz3\S+' | xargs -I{} cp {} /build/libs/ 2>/dev/null || true) \
    && touch /build/libs/.keep

# --- Runtime stage ---
FROM node:22-slim

# Copy Z3 shared libs if any were dynamically linked
COPY --from=builder /build/libs/ /usr/local/lib/
RUN ldconfig 2>/dev/null || true; rm -f /usr/local/lib/.keep

# Copy binaries
COPY --from=builder /build/target/release/shatter /usr/local/bin/shatter
COPY --from=builder /build/shatter-rust/target/release/shatter-rust /usr/local/bin/shatter-rust

# OCI image labels
LABEL org.opencontainers.image.source="https://github.com/shatter-dev/shatter"
LABEL org.opencontainers.image.description="Automatic exploratory testing via concolic execution"
LABEL org.opencontainers.image.licenses="MIT"

WORKDIR /repo
ENTRYPOINT ["shatter"]
