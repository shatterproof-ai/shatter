#!/usr/bin/env bash
# Report availability of required and optional local analysis tooling.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib/common.sh
. "${SCRIPT_DIR}/lib/common.sh"

STRICT=false

usage() {
  cat <<'EOF'
Usage: ./scripts/quality/check-tooling.sh [--strict]

Reports required and optional local tools used by quality scripts.
With --strict, missing optional tools cause a non-zero exit code.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --strict)
      STRICT=true
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

missing_optional=0

check_tool() {
  local label="$1"
  local cmd="$2"
  local kind="$3"
  if has_cmd "$cmd"; then
    info "${kind}: ${label} (${cmd})"
  else
    if [[ "${kind}" == "required" ]]; then
      die "missing required tool: ${label} (${cmd})"
    fi
    warn "optional: ${label} (${cmd}) not installed"
    missing_optional=$((missing_optional + 1))
  fi
}

step "Required tooling"
check_tool "Git" "git" "required"
check_tool "Rust/Cargo" "cargo" "required"
check_tool "Node.js" "node" "required"
check_tool "npm" "npm" "required"
check_tool "Go" "go" "required"

step "Optional analyzers and hook helpers"
check_tool "beads" "bd" "optional"
check_tool "Semgrep CE" "semgrep" "optional"
check_tool "pre-commit" "pre-commit" "optional"
check_tool "reviewdog" "reviewdog" "optional"
check_tool "actionlint" "actionlint" "optional"
check_tool "lychee" "lychee" "optional"
check_tool "vale" "vale" "optional"
check_tool "markdownlint-cli2" "markdownlint-cli2" "optional"
check_tool "golangci-lint" "golangci-lint" "optional"
check_tool "staticcheck" "staticcheck" "optional"
check_tool "govulncheck" "govulncheck" "optional"
check_tool "cargo-nextest" "cargo-nextest" "optional"
check_tool "cargo-deny" "cargo-deny" "optional"
check_tool "cargo-udeps" "cargo-udeps" "optional"

if [[ "${STRICT}" == "true" && "${missing_optional}" -gt 0 ]]; then
  die "missing ${missing_optional} optional tool(s) required by strict mode"
fi

info "tooling check complete"
