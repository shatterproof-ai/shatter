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
