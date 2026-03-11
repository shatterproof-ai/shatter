#!/usr/bin/env bash
# Pre-completion gate for local agent sessions before declaring work done.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib/common.sh
. "${SCRIPT_DIR}/lib/common.sh"

WITH_E2E=false

usage() {
  cat <<'EOF'
Usage: ./scripts/quality/pre-completion.sh [--e2e]

Runs the standard local quality gates and prints a short git status summary.
This script is intended for humans and agents before they announce completion.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
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

aggregate_args=()
if [[ "${WITH_E2E}" == "true" ]]; then
  aggregate_args+=(--e2e)
fi

run_cmd "Aggregate quality gates" "${SCRIPT_DIR}/check-all.sh" "${aggregate_args[@]}"

step "Git status"
(
  cd "${REPO_ROOT}"
  git status --short
)

info "Pre-completion checks complete"
