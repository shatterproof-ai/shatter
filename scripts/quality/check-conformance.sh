#!/usr/bin/env bash
# Run protocol conformance tests across all available frontends.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib/common.sh
. "${SCRIPT_DIR}/lib/common.sh"

STRICT_OPTIONAL=false

usage() {
  cat <<'EOF'
Usage: ./scripts/quality/check-conformance.sh [--strict-optional]

Spawns each frontend subprocess (TS, Go, Rust, noop), sends identical
protocol requests, and verifies responses match structurally.

Requires: python3, pyyaml (pip package)
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
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

if ! has_cmd python3; then
  if [[ "${STRICT_OPTIONAL}" == "true" ]]; then
    die "python3 required by strict mode"
  fi
  skip "Conformance testing (missing python3)"
  exit 0
fi

if ! python3 -c "import yaml" 2>/dev/null; then
  if [[ "${STRICT_OPTIONAL}" == "true" ]]; then
    die "pyyaml package required by strict mode (pip install pyyaml)"
  fi
  skip "Conformance testing (missing pyyaml package)"
  exit 0
fi

run_cmd "Protocol conformance testing" python3 \
    "${REPO_ROOT}/protocol/conformance/conformance_harness.py"

info "Conformance checks complete"
