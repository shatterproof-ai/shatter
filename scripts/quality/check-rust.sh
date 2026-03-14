#!/usr/bin/env bash
# Run Rust quality gates for local development or CI.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib/common.sh
. "${SCRIPT_DIR}/lib/common.sh"

RUN_CLIPPY=true
RUN_E2E=false
RUN_DENY=false
RUN_UDEPS=false
USE_NEXTEST=auto
STRICT_OPTIONAL=false

usage() {
  cat <<'EOF'
Usage: ./scripts/quality/check-rust.sh [options]

Options:
  --no-clippy         Skip cargo clippy
  --e2e               Run the e2e_concolic test target
  --deny              Run cargo-deny if installed
  --udeps             Run cargo-udeps if installed
  --nextest           Prefer cargo-nextest for the main Rust test run
  --no-nextest        Force cargo test even if nextest is available
  --strict-optional   Fail if an optional tool requested above is missing
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-clippy)
      RUN_CLIPPY=false
      ;;
    --e2e)
      RUN_E2E=true
      ;;
    --deny)
      RUN_DENY=true
      ;;
    --udeps)
      RUN_UDEPS=true
      ;;
    --nextest)
      USE_NEXTEST=true
      ;;
    --no-nextest)
      USE_NEXTEST=false
      ;;
    --strict-optional)
      STRICT_OPTIONAL=true
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown option: $1"
      ;;
  esac
  shift
done

require_cmd cargo "install Rust toolchain"

# Resolve nextest availability: auto-detect unless explicitly forced
HAS_NEXTEST=false
if has_cmd cargo-nextest; then
  HAS_NEXTEST=true
fi

if [[ "${USE_NEXTEST}" == "auto" ]]; then
  USE_NEXTEST="${HAS_NEXTEST}"
fi

if [[ "${USE_NEXTEST}" == "true" ]]; then
  if [[ "${HAS_NEXTEST}" == "true" ]]; then
    info "Using cargo-nextest for parallel test execution"
    run_cmd "Rust tests (cargo nextest)" cargo nextest run
  elif [[ "${STRICT_OPTIONAL}" == "true" ]]; then
    die "cargo-nextest requested but not installed"
  else
    warn "cargo-nextest not installed; falling back to cargo test"
    run_cmd "Rust tests (cargo test)" cargo test
  fi
else
  info "Using cargo test (install cargo-nextest for faster runs)"
  run_cmd "Rust tests (cargo test)" cargo test
fi

if [[ "${RUN_CLIPPY}" == "true" ]]; then
  run_cmd "Rust lint (cargo clippy)" cargo clippy -- -D warnings
fi

# Standalone Rust frontend crates (excluded from workspace)
if [[ "${USE_NEXTEST}" == "true" && "${HAS_NEXTEST}" == "true" ]]; then
  run_in_dir shatter-rust "shatter-rust tests (cargo nextest)" cargo nextest run
  run_in_dir shatter-rust-runtime "shatter-rust-runtime tests (cargo nextest)" cargo nextest run
else
  run_in_dir shatter-rust "shatter-rust tests" cargo test
  run_in_dir shatter-rust-runtime "shatter-rust-runtime tests" cargo test
fi

if [[ "${RUN_CLIPPY}" == "true" ]]; then
  run_in_dir shatter-rust "shatter-rust lint (cargo clippy)" cargo clippy -- -D warnings
  run_in_dir shatter-rust-runtime "shatter-rust-runtime lint (cargo clippy)" cargo clippy -- -D warnings
fi

if [[ "${RUN_E2E}" == "true" ]]; then
  run_cmd "Rust E2E tests" cargo test --test e2e_concolic
fi

if [[ "${RUN_DENY}" == "true" ]]; then
  if has_cmd cargo-deny; then
    run_cmd "Rust dependency policy (cargo deny)" cargo deny check
  elif [[ "${STRICT_OPTIONAL}" == "true" ]]; then
    die "cargo-deny requested but not installed"
  else
    skip "Rust dependency policy (missing cargo-deny)"
  fi
fi

if [[ "${RUN_UDEPS}" == "true" ]]; then
  if has_cmd cargo-udeps; then
    run_cmd "Rust unused dependency scan (cargo udeps)" cargo udeps --all-targets
  elif [[ "${STRICT_OPTIONAL}" == "true" ]]; then
    die "cargo-udeps requested but not installed"
  else
    skip "Rust unused dependency scan (missing cargo-udeps)"
  fi
fi

info "Rust checks complete"
