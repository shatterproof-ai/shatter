#!/usr/bin/env bash
set -euo pipefail

echo "==> Installing Rust dependencies..."
cargo fetch

echo "==> Installing TypeScript dependencies..."
(cd shatter-ts && npm ci)

echo "==> Installing Go dependencies..."
(cd shatter-go && go mod download)

echo "==> Running quick validation..."
cargo check
echo "✓ Devcontainer ready"
