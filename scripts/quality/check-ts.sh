#!/usr/bin/env bash
# Run TypeScript frontend quality gates for local development or CI.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib/common.sh
. "${SCRIPT_DIR}/lib/common.sh"

RUN_TESTS=true
RUN_DEPCRUISE=false
RUN_KNIP=false
STRICT_OPTIONAL=false
TEST_ARGS=(-- --runInBand)

usage() {
  cat <<'EOF'
Usage: ./scripts/quality/check-ts.sh [options]

Options:
  --no-tests          Skip npm test
  --depcruise         Run dependency-cruiser if installed and configured
  --knip              Run Knip if installed
  --strict-optional   Fail if an optional tool requested above is missing
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-tests)
      RUN_TESTS=false
      ;;
    --depcruise)
      RUN_DEPCRUISE=true
      ;;
    --knip)
      RUN_KNIP=true
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

require_cmd npm "install Node.js and npm"

run_in_dir "shatter-ts" "TypeScript build" npm run build

if [[ "${RUN_TESTS}" == "true" ]]; then
  run_in_dir "shatter-ts" "TypeScript tests" npm test "${TEST_ARGS[@]}"
fi

if [[ "${RUN_DEPCRUISE}" == "true" ]]; then
  depcruise_config=""
  if [[ -f "${REPO_ROOT}/shatter-ts/.dependency-cruiser.cjs" ]]; then
    depcruise_config=".dependency-cruiser.cjs"
  elif [[ -f "${REPO_ROOT}/shatter-ts/.dependency-cruiser.js" ]]; then
    depcruise_config=".dependency-cruiser.js"
  fi

  if [[ -n "${depcruise_config}" ]]; then
    if has_cmd npx; then
      run_in_dir "shatter-ts" "TypeScript dependency rules" npx depcruise --validate "${depcruise_config}" src
    elif [[ "${STRICT_OPTIONAL}" == "true" ]]; then
      die "dependency-cruiser requested but npx is unavailable"
    else
      skip "TypeScript dependency rules (missing npx)"
    fi
  elif [[ "${STRICT_OPTIONAL}" == "true" ]]; then
    die "dependency-cruiser requested but no config file exists in shatter-ts/"
  else
    skip "TypeScript dependency rules (no dependency-cruiser config yet)"
  fi
fi

if [[ "${RUN_KNIP}" == "true" ]]; then
  if has_cmd npx; then
    run_in_dir "shatter-ts" "TypeScript unused code scan" npx knip
  elif [[ "${STRICT_OPTIONAL}" == "true" ]]; then
    die "Knip requested but npx is unavailable"
  else
    skip "TypeScript unused code scan (missing npx)"
  fi
fi

info "TypeScript checks complete"
