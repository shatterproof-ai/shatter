#!/usr/bin/env bash
# Aggregate quality runner for local development and future CI jobs.
# Runs independent quality lanes in parallel by default for faster feedback.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib/common.sh
. "${SCRIPT_DIR}/lib/common.sh"

STRICT_OPTIONAL=false
WITH_E2E=false
PARALLEL=true

usage() {
  cat <<'EOF'
Usage: ./scripts/quality/check-all.sh [options]

Options:
  --strict-optional   Fail if optional analysis tools are missing
  --e2e               Include the Rust e2e_concolic test target
  --parallel          Run independent lanes in parallel (default)
  --no-parallel       Force sequential execution (useful for debugging)
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --strict-optional)
      STRICT_OPTIONAL=true
      ;;
    --e2e)
      WITH_E2E=true
      ;;
    --parallel)
      PARALLEL=true
      ;;
    --no-parallel)
      PARALLEL=false
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

tooling_args=()
rust_args=()
go_args=()
docs_args=()
schema_args=()
meta_args=()

if [[ "${STRICT_OPTIONAL}" == "true" ]]; then
  tooling_args+=(--strict)
  rust_args+=(--strict-optional --deny)
  go_args+=(--strict-optional --golangci-lint --staticcheck --govulncheck)
  docs_args+=(--strict-optional)
  schema_args+=(--strict-optional)
  meta_args+=(--strict-optional)
fi

if [[ "${WITH_E2E}" == "true" ]]; then
  rust_args+=(--e2e)
fi

# Tooling check always runs first (fast prerequisite gate).
run_cmd "Tooling inventory" "${SCRIPT_DIR}/check-tooling.sh" "${tooling_args[@]}"

if [[ "${PARALLEL}" == "false" ]]; then
  # --- Sequential mode (--no-parallel) --------------------------------------
  run_cmd "Rust quality gates"              "${SCRIPT_DIR}/check-rust.sh"        "${rust_args[@]}"
  run_cmd "TypeScript quality gates"        "${SCRIPT_DIR}/check-ts.sh"
  run_cmd "Go quality gates"               "${SCRIPT_DIR}/check-go.sh"          "${go_args[@]}"
  run_cmd "Documentation quality gates"    "${SCRIPT_DIR}/check-docs.sh"        "${docs_args[@]}"
  run_cmd "Protocol schema validation"     "${SCRIPT_DIR}/check-schemas.sh"     "${schema_args[@]}"
  run_cmd "Repository meta checks"         "${SCRIPT_DIR}/check-meta.sh"        "${meta_args[@]}"
  run_cmd "Protocol conformance testing"   "${SCRIPT_DIR}/check-conformance.sh" "${schema_args[@]}"
  info "All aggregate checks complete"
  exit 0
fi

# --- Parallel mode (default) ------------------------------------------------

overall_start=$(date +%s)

lane_names=(
  "Rust quality gates"
  "TypeScript quality gates"
  "Go quality gates"
  "Documentation quality gates"
  "Protocol schema validation"
  "Repository meta checks"
)
lane_scripts=(
  "${SCRIPT_DIR}/check-rust.sh"
  "${SCRIPT_DIR}/check-ts.sh"
  "${SCRIPT_DIR}/check-go.sh"
  "${SCRIPT_DIR}/check-docs.sh"
  "${SCRIPT_DIR}/check-schemas.sh"
  "${SCRIPT_DIR}/check-meta.sh"
)

# Temp dir for lane output; cleaned up on exit.
tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT

step "Running 6 lanes in parallel"
info "  Rust | TypeScript | Go | Docs | Schemas | Meta"

pids=()
log_files=()
time_files=()

for i in "${!lane_names[@]}"; do
  log_file="${tmpdir}/lane-${i}.log"
  time_file="${tmpdir}/lane-${i}.time"
  log_files+=("$log_file")
  time_files+=("$time_file")

  # Build per-lane argument array.
  lane_args=()
  case "$i" in
    0) lane_args=("${rust_args[@]}")   ;;
    1) lane_args=()                    ;;
    2) lane_args=("${go_args[@]}")     ;;
    3) lane_args=("${docs_args[@]}")   ;;
    4) lane_args=("${schema_args[@]}") ;;
    5) lane_args=("${meta_args[@]}")   ;;
  esac

  # Subshell: run lane, record wall-clock seconds.
  # Disable errexit inside so we always record the elapsed time.
  (
    set +e
    lane_start=$(date +%s)
    cd "${REPO_ROOT}"
    "${lane_scripts[$i]}" "${lane_args[@]}" 2>&1
    rc=$?
    lane_end=$(date +%s)
    echo $((lane_end - lane_start)) > "$time_file"
    exit "$rc"
  ) > "$log_file" 2>&1 &
  pids+=($!)
done

# Collect exit codes from all parallel lanes.
exit_codes=()
for pid in "${pids[@]}"; do
  set +e
  wait "$pid"
  exit_codes+=($?)
  set -e
done

# Print each lane's captured output with a clear separator.
any_failed=false
sequential_total=0
for i in "${!lane_names[@]}"; do
  rc="${exit_codes[$i]}"
  elapsed="?"
  if [[ -f "${time_files[$i]}" ]]; then
    elapsed=$(cat "${time_files[$i]}")
    sequential_total=$((sequential_total + elapsed))
  fi

  if [[ "$rc" -eq 0 ]]; then
    step "${lane_names[$i]} [PASS] (${elapsed}s)"
  else
    step "${lane_names[$i]} [FAIL] (${elapsed}s)"
    any_failed=true
  fi
  cat "${log_files[$i]}"
done

# Conformance runs after the parallel group (needs TS and Go frontends built).
step "Protocol conformance testing (sequential — needs built frontends)"
conf_start=$(date +%s)
set +e
(
  cd "${REPO_ROOT}"
  "${SCRIPT_DIR}/check-conformance.sh" "${schema_args[@]}"
)
conf_rc=$?
set -e
conf_end=$(date +%s)
conf_elapsed=$((conf_end - conf_start))
sequential_total=$((sequential_total + conf_elapsed))
if [[ "$conf_rc" -ne 0 ]]; then
  any_failed=true
fi

total_elapsed=$((conf_end - overall_start))
saved=$((sequential_total - total_elapsed))
if [[ "$saved" -lt 0 ]]; then saved=0; fi

# --- Summary ----------------------------------------------------------------
step "Parallel run summary"
for i in "${!lane_names[@]}"; do
  rc="${exit_codes[$i]}"
  elapsed="?"
  if [[ -f "${time_files[$i]}" ]]; then
    elapsed=$(cat "${time_files[$i]}")
  fi
  if [[ "$rc" -eq 0 ]]; then
    info "  PASS  ${lane_names[$i]} (${elapsed}s)"
  else
    error "  FAIL  ${lane_names[$i]} (${elapsed}s)"
  fi
done
if [[ "$conf_rc" -eq 0 ]]; then
  info "  PASS  Protocol conformance testing (${conf_elapsed}s)"
else
  error "  FAIL  Protocol conformance testing (${conf_elapsed}s)"
fi

info "Total wall-clock: ${total_elapsed}s  (sequential estimate: ${sequential_total}s, saved ~${saved}s)"

if [[ "${any_failed}" == "true" ]]; then
  die "One or more quality lanes failed (see above)"
fi

info "All aggregate checks complete"
