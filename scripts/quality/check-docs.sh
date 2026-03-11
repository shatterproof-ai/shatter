#!/usr/bin/env bash
# Run documentation quality gates for local development or CI.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib/common.sh
. "${SCRIPT_DIR}/lib/common.sh"

STRICT_OPTIONAL=false
DOC_TARGETS=(
  README.md
  AGENTS.md
  CLAUDE.md
  docs
)

usage() {
  cat <<'EOF'
Usage: ./scripts/quality/check-docs.sh [--strict-optional]

Runs doc-oriented checks using whichever open-source tools are installed.
With --strict-optional, missing optional tools become failures.
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

step "Documentation file existence sanity checks"
[[ -f "${REPO_ROOT}/README.md" ]] || die "missing README.md"
[[ -f "${REPO_ROOT}/AGENTS.md" ]] || die "missing AGENTS.md"
[[ -f "${REPO_ROOT}/CLAUDE.md" ]] || die "missing CLAUDE.md"
[[ -d "${REPO_ROOT}/docs" ]] || die "missing docs/"
info "core documentation files present"

if has_cmd markdownlint-cli2; then
  run_cmd "Markdown lint" markdownlint-cli2 "${DOC_TARGETS[@]}"
elif [[ "${STRICT_OPTIONAL}" == "true" ]]; then
  die "markdownlint-cli2 required by strict mode"
else
  skip "Markdown lint (missing markdownlint-cli2)"
fi

if has_cmd vale; then
  run_cmd "Vale prose lint" vale "${DOC_TARGETS[@]}"
elif [[ "${STRICT_OPTIONAL}" == "true" ]]; then
  die "Vale required by strict mode"
else
  skip "Vale prose lint (missing vale)"
fi

if has_cmd lychee; then
  run_cmd "Markdown link check" lychee --no-progress "${DOC_TARGETS[@]}"
elif [[ "${STRICT_OPTIONAL}" == "true" ]]; then
  die "lychee required by strict mode"
else
  skip "Markdown link check (missing lychee)"
fi

info "Documentation checks complete"
