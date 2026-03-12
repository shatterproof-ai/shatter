#!/usr/bin/env bash
# Aggregate quality runner for local development and future CI jobs.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib/common.sh
. "${SCRIPT_DIR}/lib/common.sh"

STRICT_OPTIONAL=false
WITH_E2E=false

usage() {
  cat <<'EOF'
Usage: ./scripts/quality/check-all.sh [options]

Options:
  --strict-optional   Fail if optional analysis tools are missing
  --e2e               Include the Rust e2e_concolic test target
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --strict-optional)
      STRICT_OPTIONAL=true
      ;;
    --e2e)
      WITH_E2E=true
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

tooling_args=()
rust_args=()
go_args=()
docs_args=()
schema_args=()
meta_args=()

if [[ "${STRICT_OPTIONAL}" == "true" ]]; then
  tooling_args+=(--strict)
  rust_args+=(--strict-optional --deny)
  go_args+=(--strict-optional --golangci-lint --staticcheck --govulncheck)
  docs_args+=(--strict-optional)
  schema_args+=(--strict-optional)
  meta_args+=(--strict-optional)
fi

if [[ "${WITH_E2E}" == "true" ]]; then
  rust_args+=(--e2e)
fi

run_cmd "Tooling inventory" "${SCRIPT_DIR}/check-tooling.sh" "${tooling_args[@]}"
run_cmd "Rust quality gates" "${SCRIPT_DIR}/check-rust.sh" "${rust_args[@]}"
run_cmd "TypeScript quality gates" "${SCRIPT_DIR}/check-ts.sh"
run_cmd "Go quality gates" "${SCRIPT_DIR}/check-go.sh" "${go_args[@]}"
run_cmd "Documentation quality gates" "${SCRIPT_DIR}/check-docs.sh" "${docs_args[@]}"
run_cmd "Protocol schema validation" "${SCRIPT_DIR}/check-schemas.sh" "${schema_args[@]}"
run_cmd "Protocol conformance testing" "${SCRIPT_DIR}/check-conformance.sh" "${schema_args[@]}"
run_cmd "Repository meta checks" "${SCRIPT_DIR}/check-meta.sh" "${meta_args[@]}"

info "All aggregate checks complete"
