#!/usr/bin/env bash
# Run repository meta checks for workflows and semantic guardrails.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib/common.sh
. "${SCRIPT_DIR}/lib/common.sh"

STRICT_OPTIONAL=false
SEMGREP_CONFIG="${SEMGREP_CONFIG:-.semgrep/shatter.yml}"

usage() {
  cat <<'EOF'
Usage: ./scripts/quality/check-meta.sh [--strict-optional]

Runs repository meta checks:
  - GitHub workflow linting via actionlint
  - Semgrep CE if a repository config exists
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

if [[ -d "${REPO_ROOT}/.github/workflows" ]]; then
  if has_cmd actionlint; then
    run_cmd "GitHub workflow lint" actionlint
  elif [[ "${STRICT_OPTIONAL}" == "true" ]]; then
    die "actionlint required by strict mode"
  else
    skip "GitHub workflow lint (missing actionlint)"
  fi
else
  skip "GitHub workflow lint (no .github/workflows directory)"
fi

if [[ -f "${REPO_ROOT}/${SEMGREP_CONFIG}" ]]; then
  if has_cmd semgrep; then
    run_cmd "Semgrep CE" semgrep --config "${SEMGREP_CONFIG}" "${REPO_ROOT}"
  elif [[ "${STRICT_OPTIONAL}" == "true" ]]; then
    die "Semgrep required by strict mode"
  else
    skip "Semgrep CE (missing semgrep)"
  fi
else
  skip "Semgrep CE (no config at ${SEMGREP_CONFIG})"
fi

info "Meta checks complete"
