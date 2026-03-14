#!/usr/bin/env bash
# Aggregate quality runner for local development and future CI jobs.
#
# Modes:
#   --fast   Lightweight gate for routine pre-push: clippy + workspace cargo test,
#            npm test (skip build if dist/ is fresh), go test (skip vet).
#            Skips standalone Rust crates, docs, schemas, conformance, meta checks.
#            Target: <60s on warm builds.
#
#   --full   (default, or no flag) Everything as today — the full suite.
#            Use before merge, in CI, or with SHATTER_FULL_PUSH=1.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib/common.sh
. "${SCRIPT_DIR}/lib/common.sh"

STRICT_OPTIONAL=false
WITH_E2E=false
PATH_AWARE=false
FAST_MODE=false

usage() {
  cat <<'EOF'
Usage: ./scripts/quality/check-all.sh [options]

Options:
  --fast              Lightweight gate (clippy + tests, skip docs/schemas/meta)
  --full              Full suite (default; also disables --path-aware and --fast)
  --strict-optional   Fail if optional analysis tools are missing
  --e2e               Include the Rust e2e_concolic test target
  --path-aware        Only run lanes affected by changed files (vs merge base)
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --fast)
      FAST_MODE=true
      ;;
    --full)
      FAST_MODE=false
      PATH_AWARE=false
      ;;
    --strict-optional)
      STRICT_OPTIONAL=true
      ;;
    --e2e)
      WITH_E2E=true
      ;;
    --path-aware)
      PATH_AWARE=true
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

if [[ "${FAST_MODE}" == "true" ]]; then
  info "Running in FAST mode (lightweight pre-push gate)"

  # Rust: clippy + workspace tests only (skip standalone crates)
  run_cmd "Rust lint (cargo clippy)" cargo clippy -- -D warnings
  run_cmd "Rust tests (cargo test)" cargo test

  # TypeScript: skip build if dist/ is newer than src/
  ts_dir="${REPO_ROOT}/shatter-ts"
  if [[ -d "${ts_dir}/dist" ]]; then
    # Find newest file in src/ and dist/ to decide whether to rebuild
    src_newest=$(find "${ts_dir}/src" -type f -printf '%T@\n' 2>/dev/null | sort -rn | head -1)
    dist_newest=$(find "${ts_dir}/dist" -type f -printf '%T@\n' 2>/dev/null | sort -rn | head -1)
    if [[ -n "${dist_newest}" && -n "${src_newest}" ]] && \
       awk "BEGIN {exit !(${dist_newest} >= ${src_newest})}"; then
      skip "TypeScript build (dist/ is up to date)"
    else
      run_in_dir "shatter-ts" "TypeScript build" npm run build
    fi
  else
    run_in_dir "shatter-ts" "TypeScript build" npm run build
  fi
  run_in_dir "shatter-ts" "TypeScript tests" npm test -- --runInBand

  # Go: tests only (skip vet and optional linters)
  run_in_dir "shatter-go" "Go tests" go test ./...

  info "Fast checks complete"
  exit 0
fi

# --- Full mode (default) ---

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
