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
QUALITY_VERSION_MARKER="# SHATTER QUALITY TEMPLATE VERSION: 2"

has_shatter_section() {
  grep -qF "${BEGIN_MARKER}" "$1" 2>/dev/null &&
    grep -qF "${QUALITY_VERSION_MARKER}" "$1" 2>/dev/null &&
    grep -qF "${END_MARKER}" "$1" 2>/dev/null
}

has_any_shatter_section() {
  grep -qF "${BEGIN_MARKER}" "$1" 2>/dev/null
}

install_hook() {
  local hook_name="$1"
  local hook_body="$2"
  local hook_file="${HOOKS_DIR}/${hook_name}"

  # --force and stale templates both replace the managed section in place.
  if [[ "${FORCE}" == "true" ]] && has_any_shatter_section "${hook_file}"; then
    sed -i "/${BEGIN_MARKER}/,/${END_MARKER}/d" "${hook_file}"
  fi

  if has_shatter_section "${hook_file}"; then
    echo "[ok]   ${hook_name}: Shatter quality section present"
    return 0
  fi

  if "${CHECK_ONLY}"; then
    echo "[miss] ${hook_name}: Shatter quality section missing or stale"
    return 1
  fi

  if has_any_shatter_section "${hook_file}"; then
    sed -i "/${BEGIN_MARKER}/,/${END_MARKER}/d" "${hook_file}"
  fi

  # Create the hook file with a shebang if it doesn't exist
  if [[ ! -f "${hook_file}" ]]; then
    printf '#!/usr/bin/env sh\n' > "${hook_file}"
  fi

  chmod +x "${hook_file}"

  # Append the quality section
  cat >> "${hook_file}" <<HOOK
${BEGIN_MARKER}
${QUALITY_VERSION_MARKER}
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
#   other refs/heads/*                (non-deletion) -> affected
#   tags / other non-head refs, and any deletion (all-zero local sha) -> no gate
#   empty/blank stdin -> affected (conservative fallback)
#   malformed input (wrong field count, non-hex/wrong-length sha) -> exit 64
# Input validation and gate classification always run, independent of
# whether Taskfile.yml/task are available — only the actual gate
# invocation is skipped when those preconditions are missing.
# Set SHATTER_FULL_PUSH=1 to force the full suite on any push.
#
# After the real gate above runs (unchanged), an observational shadow check
# (scripts/receipt-shadow-check.py, str-35vtk.25) records one receipt_shadow
# event to the str-35vtk.18 gate event log: it independently asks whether a
# valid local-tier receipt already covered this exact push, purely for later
# analysis (str-35vtk.26). It never influences the real gate above, and any
# failure in it (including failure to append the event) only warns on
# stderr -- it can never skip or fail the real gate or any other hook
# section.
PRE_PUSH_BODY='shatter_is_sha1() {
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
SHATTER_AFFECTED_HEADS=""
SHATTER_ALL_LINES=""

while IFS= read -r shatter_line || [ -n "${shatter_line}" ]; do
  [ -z "${shatter_line}" ] && continue
  SHATTER_LINE_COUNT=$((SHATTER_LINE_COUNT + 1))
  SHATTER_ALL_LINES="${SHATTER_ALL_LINES}${shatter_line}
"

  set -f
  # shellcheck disable=SC2086
  set -- ${shatter_line}
  set +f
  if [ "$#" -ne 4 ]; then
    echo "[shatter] malformed pre-push input: expected 4 fields, got $#" >&2
    exit 64
  fi
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
      case " ${SHATTER_AFFECTED_HEADS} " in
        *" ${shatter_local_sha} "*) : ;;
        *) SHATTER_AFFECTED_HEADS="${SHATTER_AFFECTED_HEADS:+${SHATTER_AFFECTED_HEADS} }${shatter_local_sha}" ;;
      esac
      ;;
    *) : ;; # tags / other non-head refs: no gate
  esac
done

if [ "${SHATTER_FULL_PUSH:-0}" = "1" ]; then
  PUSH_TASK="check"
elif [ "${SHATTER_LINE_COUNT}" -eq 0 ]; then
  PUSH_TASK="affected"
else
  case "${SHATTER_GATE_RANK}" in
    2) PUSH_TASK="check" ;;
    1) PUSH_TASK="affected" ;;
    *) PUSH_TASK="" ;;
  esac
fi

SHATTER_GATE_RAN=0
SHATTER_GATE_EXIT=0

if [ -z "${PUSH_TASK}" ]; then
  echo "[shatter] No product gate required for this push."
elif [ -f "Taskfile.yml" ] && command -v task >/dev/null 2>&1; then
  echo "[shatter] Running task ${PUSH_TASK}..."
  SHATTER_GATE_RAN=1
  if [ "${PUSH_TASK}" = "affected" ]; then
    AFFECTED_HEADS="${SHATTER_AFFECTED_HEADS}" task "${PUSH_TASK}" 2>&1
    SHATTER_GATE_EXIT=$?
  else
    task "${PUSH_TASK}" 2>&1
    SHATTER_GATE_EXIT=$?
  fi
else
  echo "[shatter] Taskfile.yml or task command unavailable; skipping ${PUSH_TASK} gate."
fi

# str-35vtk.25: observational shadow check. Runs after the real gate above
# completes and never affects it -- any failure here (missing python3,
# missing script, non-zero exit, event-log append failure) is swallowed
# and only warned about on stderr.
if command -v python3 >/dev/null 2>&1 && [ -f "scripts/receipt-shadow-check.py" ]; then
  if ! printf "%s" "${SHATTER_ALL_LINES}" | python3 scripts/receipt-shadow-check.py \
    --worktree "$(pwd)" \
    --push-task "${PUSH_TASK}" \
    --gate-ran "${SHATTER_GATE_RAN}" \
    --gate-exit "${SHATTER_GATE_EXIT}" >&2; then
    echo "[shatter] warning: receipt shadow check did not complete cleanly" >&2
  fi
fi

if [ "${SHATTER_GATE_EXIT}" -ne 0 ]; then
  exit 1
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
