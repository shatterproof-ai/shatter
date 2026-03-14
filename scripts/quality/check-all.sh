#!/usr/bin/env bash
# Aggregate quality runner for local development and future CI jobs.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib/common.sh
. "${SCRIPT_DIR}/lib/common.sh"

STRICT_OPTIONAL=false
WITH_E2E=false
PATH_AWARE=false

usage() {
  cat <<'EOF'
Usage: ./scripts/quality/check-all.sh [options]

Options:
  --strict-optional   Fail if optional analysis tools are missing
  --e2e               Include the Rust e2e_concolic test target
  --path-aware        Only run lanes affected by changed files (vs merge base)
  --full              Run all lanes (default; explicit override for --path-aware)
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
    --path-aware)
      PATH_AWARE=true
      ;;
    --full)
      PATH_AWARE=false
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

# When --path-aware is set, classify changed paths and skip unaffected lanes.
# Otherwise (default / --full), all lanes run.
if [[ "${PATH_AWARE}" == "true" ]]; then
  classify_changed_paths
fi

# Tooling inventory always runs — it's cheap and validates the environment
run_cmd "Tooling inventory" "${SCRIPT_DIR}/check-tooling.sh" "${tooling_args[@]}"

if lane_enabled rust; then
  run_cmd "Rust quality gates" "${SCRIPT_DIR}/check-rust.sh" "${rust_args[@]}"
else
  skip "Rust quality gates (no Rust changes)"
fi

if lane_enabled ts; then
  run_cmd "TypeScript quality gates" "${SCRIPT_DIR}/check-ts.sh"
else
  skip "TypeScript quality gates (no TS changes)"
fi

if lane_enabled go; then
  run_cmd "Go quality gates" "${SCRIPT_DIR}/check-go.sh" "${go_args[@]}"
else
  skip "Go quality gates (no Go changes)"
fi

if lane_enabled docs; then
  run_cmd "Documentation quality gates" "${SCRIPT_DIR}/check-docs.sh" "${docs_args[@]}"
else
  skip "Documentation quality gates (no docs changes)"
fi

if lane_enabled schemas; then
  run_cmd "Protocol schema validation" "${SCRIPT_DIR}/check-schemas.sh" "${schema_args[@]}"
else
  skip "Protocol schema validation (no schema changes)"
fi

if lane_enabled conformance; then
  run_cmd "Protocol conformance testing" "${SCRIPT_DIR}/check-conformance.sh" "${schema_args[@]}"
else
  skip "Protocol conformance testing (no protocol changes)"
fi

if lane_enabled meta; then
  run_cmd "Repository meta checks" "${SCRIPT_DIR}/check-meta.sh" "${meta_args[@]}"
else
  skip "Repository meta checks (no meta changes)"
fi

info "All aggregate checks complete"
