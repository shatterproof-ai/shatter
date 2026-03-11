#!/usr/bin/env bash
# Run Go frontend quality gates for local development or CI.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib/common.sh
. "${SCRIPT_DIR}/lib/common.sh"

RUN_VET=true
RUN_GOLANGCI=false
RUN_STATICCHECK=false
RUN_GOVULNCHECK=false
STRICT_OPTIONAL=false

usage() {
  cat <<'EOF'
Usage: ./scripts/quality/check-go.sh [options]

Options:
  --no-vet            Skip go vet
  --golangci-lint     Run golangci-lint if installed
  --staticcheck       Run staticcheck if installed
  --govulncheck       Run govulncheck if installed
  --strict-optional   Fail if an optional tool requested above is missing
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --no-vet)
      RUN_VET=false
      ;;
    --golangci-lint)
      RUN_GOLANGCI=true
      ;;
    --staticcheck)
      RUN_STATICCHECK=true
      ;;
    --govulncheck)
      RUN_GOVULNCHECK=true
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

require_cmd go "install the Go toolchain"

run_in_dir "shatter-go" "Go tests" go test ./...

if [[ "${RUN_VET}" == "true" ]]; then
  run_in_dir "shatter-go" "Go vet" go vet ./...
fi

if [[ "${RUN_GOLANGCI}" == "true" ]]; then
  if has_cmd golangci-lint; then
    run_in_dir "shatter-go" "golangci-lint" golangci-lint run ./...
  elif [[ "${STRICT_OPTIONAL}" == "true" ]]; then
    die "golangci-lint requested but not installed"
  else
    skip "golangci-lint (missing golangci-lint)"
  fi
fi

if [[ "${RUN_STATICCHECK}" == "true" ]]; then
  if has_cmd staticcheck; then
    run_in_dir "shatter-go" "staticcheck" staticcheck ./...
  elif [[ "${STRICT_OPTIONAL}" == "true" ]]; then
    die "staticcheck requested but not installed"
  else
    skip "staticcheck (missing staticcheck)"
  fi
fi

if [[ "${RUN_GOVULNCHECK}" == "true" ]]; then
  if has_cmd govulncheck; then
    run_in_dir "shatter-go" "govulncheck" govulncheck ./...
  elif [[ "${STRICT_OPTIONAL}" == "true" ]]; then
    die "govulncheck requested but not installed"
  else
    skip "govulncheck (missing govulncheck)"
  fi
fi

info "Go checks complete"
