#!/usr/bin/env bash
# Validate all protocol fixtures against JSON schemas.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib/common.sh
. "${SCRIPT_DIR}/lib/common.sh"

STRICT_OPTIONAL=false

usage() {
  cat <<'EOF'
Usage: ./scripts/quality/check-schemas.sh [--strict-optional]

Validates protocol fixtures against JSON schemas:
  - Valid fixtures must pass schema validation
  - Invalid fixtures must fail schema validation
  - Error fixtures must pass response schema validation

Requires: python3, jsonschema (pip package)
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
  skip "Schema validation (missing python3)"
  exit 0
fi

if ! python3 -c "import jsonschema" 2>/dev/null; then
  if [[ "${STRICT_OPTIONAL}" == "true" ]]; then
    die "jsonschema package required by strict mode (pip install jsonschema)"
  fi
  skip "Schema validation (missing jsonschema package)"
  exit 0
fi

run_cmd "Protocol schema validation" python3 "${REPO_ROOT}/protocol/schemas/test_schema_validation.py"

info "Schema checks complete"
