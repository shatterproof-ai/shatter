#!/usr/bin/env bash
# Bootstrap local git hooks that delegate to repo-owned quality scripts.
#
# Idempotent — safe to run multiple times. Preserves existing hook content
# (e.g. Beads integration) and appends a guarded "SHATTER QUALITY" section.
#
# Usage:
#   ./scripts/setup-hooks.sh          # install hooks
#   ./scripts/setup-hooks.sh --force  # replace existing Shatter section
#   ./scripts/setup-hooks.sh --check  # report status without modifying

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
# Resolve the real hooks dir via git so this works from linked worktrees too,
# where ${REPO_ROOT}/.git is a gitdir-pointer file rather than a directory.
GIT_COMMON_DIR="$(git -C "${REPO_ROOT}" rev-parse --git-common-dir)"
case "${GIT_COMMON_DIR}" in
  /*) ;; # already absolute
  *) GIT_COMMON_DIR="${REPO_ROOT}/${GIT_COMMON_DIR}" ;;
esac
HOOKS_DIR="${GIT_COMMON_DIR}/hooks"

CHECK_ONLY=false
FORCE=false
for arg in "$@"; do
  case "${arg}" in
    --check) CHECK_ONLY=true ;;
    --force) FORCE=true ;;
  esac
done

BEGIN_MARKER="# --- BEGIN SHATTER QUALITY ---"
END_MARKER="# --- END SHATTER QUALITY ---"
ENV_BEGIN_MARKER="# --- BEGIN SHATTER HOOK ENV ---"
ENV_END_MARKER="# --- END SHATTER HOOK ENV ---"
# The single-quoted value is emitted literally into each installed hook.
# shellcheck disable=SC2016
BEADS_HOOK_TIMEOUT_LINE='export BEADS_HOOK_TIMEOUT="${BEADS_HOOK_TIMEOUT:-30}"'
BEADS_HOOKS=(pre-commit post-merge pre-push post-checkout prepare-commit-msg)

has_shatter_section() {
  grep -qF "${BEGIN_MARKER}" "$1" 2>/dev/null
}

has_hook_env_section() {
  grep -qF "${ENV_BEGIN_MARKER}" "$1" 2>/dev/null &&
    grep -qF "${BEADS_HOOK_TIMEOUT_LINE}" "$1" 2>/dev/null &&
    grep -qF "${ENV_END_MARKER}" "$1" 2>/dev/null
}

install_hook_env() {
  local hook_name="$1"
  local hook_file="${HOOKS_DIR}/${hook_name}"

  if has_hook_env_section "${hook_file}"; then
    echo "[ok]   ${hook_name}: Shatter hook environment present"
    return 0
  fi
  if "${CHECK_ONLY}"; then
    echo "[miss] ${hook_name}: Shatter hook environment missing or stale"
    return 1
  fi

  mkdir -p "${HOOKS_DIR}"
  if [[ ! -f "${hook_file}" ]]; then
    printf '#!/usr/bin/env sh\n' > "${hook_file}"
  fi
  if grep -qF "${ENV_BEGIN_MARKER}" "${hook_file}" 2>/dev/null; then
    sed -i "/${ENV_BEGIN_MARKER}/,/${ENV_END_MARKER}/d" "${hook_file}"
  fi

  local hook_tmp
  hook_tmp="$(mktemp "${hook_file}.XXXXXX")"
  {
    IFS= read -r first_line || true
    if [[ "${first_line}" == '#!'* ]]; then
      printf '%s\n' "${first_line}"
    else
      printf '#!/usr/bin/env sh\n'
      [[ -z "${first_line}" ]] || printf '%s\n' "${first_line}"
    fi
    printf '%s\n' \
      "${ENV_BEGIN_MARKER}" \
      '# Managed by scripts/setup-hooks.sh — do not edit between markers.' \
      "${BEADS_HOOK_TIMEOUT_LINE}" \
      "${ENV_END_MARKER}"
    cat
  } < "${hook_file}" > "${hook_tmp}"
  chmod +x "${hook_tmp}"
  mv "${hook_tmp}" "${hook_file}"
  echo "[add]  ${hook_name}: 30s Beads hook timeout installed"
}

install_hook() {
  local hook_name="$1"
  local hook_body="$2"
  local hook_file="${HOOKS_DIR}/${hook_name}"

  # --force: strip existing section before re-adding
  if [[ "${FORCE}" == "true" ]] && has_shatter_section "${hook_file}"; then
    sed -i "/${BEGIN_MARKER}/,/${END_MARKER}/d" "${hook_file}"
  fi

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

# Pre-commit: targeted Rust checks on staged files only
PRE_COMMIT_BODY='if [ -f "scripts/precommit-rust.sh" ]; then
  scripts/precommit-rust.sh 2>&1 || exit 1
fi'

# Pre-push: classify every ref update on stdin (git pre-push protocol:
# "<local ref> <local sha1> <remote ref> <remote sha1>") and run the
# strongest gate required across all of them.
#   refs/heads/main|refs/heads/master (non-deletion) -> check
#   other refs/heads/*                (non-deletion) -> check-fast
#   tags / other non-head refs, and any deletion (all-zero local sha) -> no gate
#   empty/blank stdin -> check-fast (conservative fallback)
#   malformed input (wrong field count, non-hex/wrong-length sha) -> exit 64
# Set SHATTER_FULL_PUSH=1 to force the full suite on any push.
PRE_PUSH_BODY='if [ -f "Taskfile.yml" ] && command -v task >/dev/null 2>&1; then
  shatter_is_sha1() {
    sha="$1"
    if [ "${#sha}" -ne 40 ]; then
      return 1
    fi
    case "${sha}" in
      *[!0-9a-f]*) return 1 ;;
    esac
    return 0
  }

  SHATTER_ZERO_SHA="0000000000000000000000000000000000000000"
  SHATTER_GATE_RANK=0
  SHATTER_LINE_COUNT=0

  while IFS= read -r shatter_line || [ -n "${shatter_line}" ]; do
    [ -z "${shatter_line}" ] && continue
    SHATTER_LINE_COUNT=$((SHATTER_LINE_COUNT + 1))

    set -f
    # shellcheck disable=SC2086
    set -- ${shatter_line}
    set +f
    if [ "$#" -ne 4 ]; then
      echo "[shatter] malformed pre-push input: expected 4 fields, got $#" >&2
      exit 64
    fi
    shatter_local_ref="$1"
    shatter_local_sha="$2"
    shatter_remote_ref="$3"
    shatter_remote_sha="$4"

    if ! shatter_is_sha1 "${shatter_local_sha}" || ! shatter_is_sha1 "${shatter_remote_sha}"; then
      echo "[shatter] malformed pre-push input: non-hex or wrong-length SHA" >&2
      exit 64
    fi

    if [ "${shatter_local_sha}" = "${SHATTER_ZERO_SHA}" ]; then
      continue # deletion: contributes no gate requirement
    fi

    case "${shatter_remote_ref}" in
      refs/heads/main|refs/heads/master)
        [ "${SHATTER_GATE_RANK}" -lt 2 ] && SHATTER_GATE_RANK=2
        ;;
      refs/heads/*)
        [ "${SHATTER_GATE_RANK}" -lt 1 ] && SHATTER_GATE_RANK=1
        ;;
      *) : ;; # tags / other non-head refs: no gate
    esac
  done

  if [ "${SHATTER_FULL_PUSH:-0}" = "1" ]; then
    PUSH_TASK="check"
  elif [ "${SHATTER_LINE_COUNT}" -eq 0 ]; then
    PUSH_TASK="check-fast"
  else
    case "${SHATTER_GATE_RANK}" in
      2) PUSH_TASK="check" ;;
      1) PUSH_TASK="check-fast" ;;
      *) PUSH_TASK="" ;;
    esac
  fi

  if [ -n "${PUSH_TASK}" ]; then
    echo "[shatter] Running task ${PUSH_TASK}..."
    task "${PUSH_TASK}" 2>&1 || exit 1
  else
    echo "[shatter] No product gate required for this push."
  fi
fi'

MISSING=0
for hook_name in "${BEADS_HOOKS[@]}"; do
  install_hook_env "${hook_name}" || MISSING=$((MISSING + 1))
done
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
