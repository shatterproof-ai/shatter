#!/usr/bin/env bash
# Shared helpers for local quality scripts and future CI entrypoints.

COMMON_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${COMMON_DIR}/../../.." && pwd)"

if [[ -t 1 ]]; then
  BOLD="$(printf '\033[1m')"
  RED="$(printf '\033[31m')"
  YELLOW="$(printf '\033[33m')"
  GREEN="$(printf '\033[32m')"
  BLUE="$(printf '\033[34m')"
  RESET="$(printf '\033[0m')"
else
  BOLD=""
  RED=""
  YELLOW=""
  GREEN=""
  BLUE=""
  RESET=""
fi

step() {
  printf '\n%s==> %s%s\n' "${BOLD}${BLUE}" "$1" "${RESET}"
}

info() {
  printf '%s[info]%s %s\n' "${GREEN}" "${RESET}" "$1"
}

warn() {
  printf '%s[warn]%s %s\n' "${YELLOW}" "${RESET}" "$1"
}

skip() {
  printf '%s[skip]%s %s\n' "${YELLOW}" "${RESET}" "$1"
}

error() {
  printf '%s[error]%s %s\n' "${RED}" "${RESET}" "$1" >&2
}

die() {
  error "$1"
  exit 1
}

has_cmd() {
  command -v "$1" >/dev/null 2>&1
}

require_cmd() {
  local cmd="$1"
  local hint="${2:-}"
  if ! has_cmd "$cmd"; then
    if [[ -n "$hint" ]]; then
      die "missing required command '${cmd}' (${hint})"
    fi
    die "missing required command '${cmd}'"
  fi
}

run_cmd() {
  local label="$1"
  shift
  step "$label"
  (
    cd "${REPO_ROOT}"
    "$@"
  )
}

run_in_dir() {
  local dir="$1"
  local label="$2"
  shift 2
  step "$label"
  (
    cd "${REPO_ROOT}/${dir}"
    "$@"
  )
}

maybe_run_cmd() {
  local cmd="$1"
  local label="$2"
  shift 2
  if has_cmd "$cmd"; then
    run_cmd "$label" "$cmd" "$@"
  else
    skip "${label} (missing ${cmd})"
  fi
}

maybe_run_in_dir() {
  local dir="$1"
  local cmd="$2"
  local label="$3"
  shift 3
  if has_cmd "$cmd"; then
    run_in_dir "$dir" "$label" "$cmd" "$@"
  else
    skip "${label} (missing ${cmd})"
  fi
}

# ---------------------------------------------------------------------------
# Path-aware lane classification
#
# Analyzes changed files between the current branch and its merge base to
# determine which quality lanes need to run. Sets global associative array
# LANES with boolean values for each lane.
#
# Path rules:
#   shatter-core/, shatter-cli/, shatter-rust/, shatter-rust-runtime/, Cargo.* → rust
#   shatter-ts/                                                                → ts
#   shatter-go/                                                                → go
#   protocol/                  → schemas, conformance, AND all language lanes
#   docs/, *.md, CLAUDE.md, AGENTS.md                                          → docs
#   .github/, scripts/                                                         → meta
#   examples/                                                                  → rust (E2E tests reference examples)
#
# Fallback: unknown paths or empty diff → all lanes enabled.
# ---------------------------------------------------------------------------

declare -gA LANES

classify_changed_paths() {
  # Initialize all lanes to false
  LANES=(
    [rust]=false
    [ts]=false
    [go]=false
    [docs]=false
    [schemas]=false
    [conformance]=false
    [meta]=false
  )

  local merge_base
  merge_base="$(git merge-base HEAD origin/main 2>/dev/null || true)"

  if [[ -z "${merge_base}" ]]; then
    warn "Could not determine merge base — running all lanes"
    _enable_all_lanes
    return
  fi

  local changed_files
  changed_files="$(git diff --name-only "${merge_base}..HEAD" 2>/dev/null || true)"

  if [[ -z "${changed_files}" ]]; then
    warn "No changed files detected — running all lanes"
    _enable_all_lanes
    return
  fi

  local has_unknown=false

  while IFS= read -r file; do
    case "${file}" in
      shatter-core/* | shatter-cli/* | shatter-rust/* | shatter-rust-runtime/* | Cargo.*)
        LANES[rust]=true
        ;;
      shatter-ts/*)
        LANES[ts]=true
        ;;
      shatter-go/*)
        LANES[go]=true
        ;;
      protocol/*)
        # Protocol changes affect all language frontends plus schema/conformance
        LANES[schemas]=true
        LANES[conformance]=true
        LANES[rust]=true
        LANES[ts]=true
        LANES[go]=true
        ;;
      docs/* | *.md)
        LANES[docs]=true
        ;;
      .github/* | scripts/*)
        LANES[meta]=true
        ;;
      examples/*)
        # E2E tests reference example files
        LANES[rust]=true
        ;;
      *)
        has_unknown=true
        ;;
    esac
  done <<< "${changed_files}"

  if [[ "${has_unknown}" == "true" ]]; then
    warn "Unclassified paths detected — running all lanes for safety"
    _enable_all_lanes
    return
  fi

  # Log which lanes are active
  local active=()
  local skipped=()
  for lane in rust ts go docs schemas conformance meta; do
    if [[ "${LANES[${lane}]}" == "true" ]]; then
      active+=("${lane}")
    else
      skipped+=("${lane}")
    fi
  done

  info "Path-aware gating: active=[${active[*]}] skipped=[${skipped[*]:-none}]"
}

_enable_all_lanes() {
  for lane in rust ts go docs schemas conformance meta; do
    LANES[${lane}]=true
  done
}

lane_enabled() {
  [[ "${LANES[${1}]:-true}" == "true" ]]
}
