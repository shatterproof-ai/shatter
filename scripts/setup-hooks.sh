#!/usr/bin/env bash
# Bootstrap local git hooks that delegate to repo-owned quality scripts.
#
# Idempotent — safe to run multiple times. Preserves existing hook content
# (e.g. Beads integration) and appends a guarded "SHATTER QUALITY" section.
#
# Usage:
#   ./scripts/setup-hooks.sh          # install hooks
#   ./scripts/setup-hooks.sh --check  # report status without modifying

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
HOOKS_DIR="${REPO_ROOT}/.git/hooks"

CHECK_ONLY=false
if [[ "${1:-}" == "--check" ]]; then
  CHECK_ONLY=true
fi

BEGIN_MARKER="# --- BEGIN SHATTER QUALITY ---"
END_MARKER="# --- END SHATTER QUALITY ---"

has_shatter_section() {
  grep -qF "${BEGIN_MARKER}" "$1" 2>/dev/null
}

install_hook() {
  local hook_name="$1"
  local hook_body="$2"
  local hook_file="${HOOKS_DIR}/${hook_name}"

  if has_shatter_section "${hook_file}"; then
    echo "[ok]   ${hook_name}: Shatter quality section present"
    return 0
  fi

  if "${CHECK_ONLY}"; then
    echo "[miss] ${hook_name}: Shatter quality section missing"
    return 1
  fi

  # Create the hook file with a shebang if it doesn't exist
  if [[ ! -f "${hook_file}" ]]; then
    printf '#!/usr/bin/env sh\n' > "${hook_file}"
  fi

  chmod +x "${hook_file}"

  # Append the quality section
  cat >> "${hook_file}" <<HOOK
${BEGIN_MARKER}
# Managed by scripts/setup-hooks.sh — do not edit between markers.
${hook_body}
${END_MARKER}
HOOK

  echo "[add]  ${hook_name}: Shatter quality section installed"
}

# Pre-commit: lightweight checks (Rust clippy on staged files)
PRE_COMMIT_BODY='if [ -f "scripts/quality/check-rust.sh" ]; then
  scripts/quality/check-rust.sh 2>&1 || exit 1
fi'

# Pre-push: fast + path-aware by default, full suite for main branch pushes.
# Set SHATTER_FULL_PUSH=1 to force the full suite on any push.
PRE_PUSH_BODY='if [ -f "scripts/quality/check-all.sh" ]; then
  PUSH_MODE="--fast --path-aware"

  # Full mode when: SHATTER_FULL_PUSH=1 or pushing to main/master
  if [ "${SHATTER_FULL_PUSH:-0}" = "1" ]; then
    PUSH_MODE="--full"
  fi

  # Detect the remote ref from stdin (pre-push hook receives lines on stdin)
  while read -r local_ref local_sha remote_ref remote_sha; do
    case "${remote_ref}" in
      refs/heads/main|refs/heads/master)
        PUSH_MODE="--full"
        ;;
    esac
  done

  echo "[shatter] Running quality gates before push (${PUSH_MODE})..."
  scripts/quality/check-all.sh ${PUSH_MODE} 2>&1 || exit 1
fi'

MISSING=0
install_hook "pre-commit" "${PRE_COMMIT_BODY}" || MISSING=$((MISSING + 1))
install_hook "pre-push" "${PRE_PUSH_BODY}" || MISSING=$((MISSING + 1))

if "${CHECK_ONLY}"; then
  if [[ "${MISSING}" -gt 0 ]]; then
    echo ""
    echo "${MISSING} hook(s) missing Shatter quality section."
    echo "Run scripts/setup-hooks.sh to install."
    exit 1
  fi
fi

echo ""
echo "Hook bootstrap complete."
