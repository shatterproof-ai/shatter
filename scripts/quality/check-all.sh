#!/usr/bin/env bash
# Aggregate quality runner for local development and future CI jobs.
#
# Modes:
#   --fast        Lightweight gate for routine pre-push: clippy + workspace cargo test,
#                 npm test (skip build if dist/ is fresh), go test (skip vet).
#                 Skips standalone Rust crates, docs, schemas, conformance, meta checks.
#                 Target: <60s on warm builds.
#
#   --full        (default, or no flag) Everything as today — the full suite.
#                 Use before merge, in CI, or with SHATTER_FULL_PUSH=1.
#
#   --path-aware  Only run lanes affected by changed files (vs merge base).
#
#   --parallel    Run independent lanes in parallel (default in full mode).
#   --no-parallel Force sequential execution (useful for debugging).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=lib/common.sh
. "${SCRIPT_DIR}/lib/common.sh"

STRICT_OPTIONAL=false
WITH_E2E=false
PATH_AWARE=false
FAST_MODE=false
PARALLEL=true

usage() {
  cat <<'EOF'
Usage: ./scripts/quality/check-all.sh [options]

Options:
  --fast              Lightweight gate (clippy + tests, skip docs/schemas/meta)
  --full              Full suite (default; also disables --path-aware and --fast)
  --strict-optional   Fail if optional analysis tools are missing
  --e2e               Include the Rust e2e_concolic test target
  --path-aware        Only run lanes affected by changed files (vs merge base)
  --parallel          Run independent lanes in parallel (default)
  --no-parallel       Force sequential execution (useful for debugging)
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --fast)
      FAST_MODE=true
      ;;
    --full)
      FAST_MODE=false
      PATH_AWARE=false
      ;;
    --strict-optional)
      STRICT_OPTIONAL=true
      ;;
    --e2e)
      WITH_E2E=true
      ;;
    --path-aware)
      PATH_AWARE=true
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

if [[ "${FAST_MODE}" == "true" ]]; then
  info "Running in FAST mode (lightweight pre-push gate)"

  # Rust: clippy + workspace tests only (skip standalone crates)
  run_cmd "Rust lint (cargo clippy)" cargo clippy -- -D warnings
  run_cmd "Rust tests (cargo test)" cargo test -- --include-ignored

  # TypeScript: skip build if dist/ is newer than src/
  ts_dir="${REPO_ROOT}/shatter-ts"
  if [[ -d "${ts_dir}/dist" ]]; then
    # Find newest file in src/ and dist/ to decide whether to rebuild
    src_newest=$(find "${ts_dir}/src" -type f -printf '%T@\n' 2>/dev/null | sort -rn | head -1)
    dist_newest=$(find "${ts_dir}/dist" -type f -printf '%T@\n' 2>/dev/null | sort -rn | head -1)
    if [[ -n "${dist_newest}" && -n "${src_newest}" ]] && \
       awk "BEGIN {exit !(${dist_newest} >= ${src_newest})}"; then
      skip "TypeScript build (dist/ is up to date)"
    else
      run_in_dir "shatter-ts" "TypeScript build" npm run build
    fi
  else
    run_in_dir "shatter-ts" "TypeScript build" npm run build
  fi
  run_in_dir "shatter-ts" "TypeScript tests" npm test -- --runInBand

  # Go: tests only (skip vet and optional linters)
  run_in_dir "shatter-go" "Go tests" go test ./...

  info "Fast checks complete"
  exit 0
fi

# --- Full mode (default) ---

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

# Full mode always runs ignored (slow) tests — they were skipped in pre-commit.
rust_args+=(--include-ignored)

# When --path-aware is set, classify changed paths and skip unaffected lanes.
# Otherwise (default / --full), all lanes run.
if [[ "${PATH_AWARE}" == "true" ]]; then
  classify_changed_paths
fi

# Tooling inventory always runs — it's cheap and validates the environment.
run_cmd "Tooling inventory" "${SCRIPT_DIR}/check-tooling.sh" "${tooling_args[@]}"

# --- Helper: check if a lane should run (path-aware gating) ---
should_run_lane() {
  local lane="$1"
  local label="$2"
  if lane_enabled "$lane"; then
    return 0
  else
    skip "${label} (no ${lane} changes)"
    return 1
  fi
}

if [[ "${PARALLEL}" == "false" ]]; then
  # --- Sequential mode (--no-parallel) --------------------------------------
  should_run_lane rust "Rust quality gates" && \
    run_cmd "Rust quality gates" "${SCRIPT_DIR}/check-rust.sh" "${rust_args[@]}"
  should_run_lane ts "TypeScript quality gates" && \
    run_cmd "TypeScript quality gates" "${SCRIPT_DIR}/check-ts.sh"
  should_run_lane go "Go quality gates" && \
    run_cmd "Go quality gates" "${SCRIPT_DIR}/check-go.sh" "${go_args[@]}"
  should_run_lane docs "Documentation quality gates" && \
    run_cmd "Documentation quality gates" "${SCRIPT_DIR}/check-docs.sh" "${docs_args[@]}"
  should_run_lane schemas "Protocol schema validation" && \
    run_cmd "Protocol schema validation" "${SCRIPT_DIR}/check-schemas.sh" "${schema_args[@]}"
  should_run_lane meta "Repository meta checks" && \
    run_cmd "Repository meta checks" "${SCRIPT_DIR}/check-meta.sh" "${meta_args[@]}"
  should_run_lane conformance "Protocol conformance testing" && \
    run_cmd "Protocol conformance testing" "${SCRIPT_DIR}/check-conformance.sh" "${schema_args[@]}"
  info "All aggregate checks complete"
  exit 0
fi

# --- Parallel mode (default) ------------------------------------------------

overall_start=$(date +%s)

lane_keys=(rust ts go docs schemas meta)
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

# Determine which lanes to run (path-aware gating).
enabled_lanes=()
for i in "${!lane_keys[@]}"; do
  if lane_enabled "${lane_keys[$i]}"; then
    enabled_lanes+=("$i")
  else
    skip "${lane_names[$i]} (no ${lane_keys[$i]} changes)"
  fi
done

if [[ ${#enabled_lanes[@]} -eq 0 ]]; then
  info "No lanes to run (all skipped by path-aware gating)"
  info "All aggregate checks complete"
  exit 0
fi

step "Running ${#enabled_lanes[@]} lanes in parallel"

pids=()
log_files=()
time_files=()
pid_to_idx=()

for i in "${enabled_lanes[@]}"; do
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
  pid_to_idx+=("$i")
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
for j in "${!pids[@]}"; do
  i="${pid_to_idx[$j]}"
  rc="${exit_codes[$j]}"
  elapsed="?"
  if [[ -f "${time_files[$j]}" ]]; then
    elapsed=$(cat "${time_files[$j]}")
    sequential_total=$((sequential_total + elapsed))
  fi

  if [[ "$rc" -eq 0 ]]; then
    step "${lane_names[$i]} [PASS] (${elapsed}s)"
  else
    step "${lane_names[$i]} [FAIL] (${elapsed}s)"
    any_failed=true
  fi
  cat "${log_files[$j]}"
done

# Conformance runs after the parallel group (needs TS and Go frontends built).
conf_rc=0
conf_elapsed=0
if lane_enabled conformance; then
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
else
  skip "Protocol conformance testing (no protocol changes)"
fi

total_end=$(date +%s)
total_elapsed=$((total_end - overall_start))
saved=$((sequential_total - total_elapsed))
if [[ "$saved" -lt 0 ]]; then saved=0; fi

# --- Summary ----------------------------------------------------------------
step "Parallel run summary"
for j in "${!pids[@]}"; do
  i="${pid_to_idx[$j]}"
  rc="${exit_codes[$j]}"
  elapsed="?"
  if [[ -f "${time_files[$j]}" ]]; then
    elapsed=$(cat "${time_files[$j]}")
  fi
  if [[ "$rc" -eq 0 ]]; then
    info "  PASS  ${lane_names[$i]} (${elapsed}s)"
  else
    error "  FAIL  ${lane_names[$i]} (${elapsed}s)"
  fi
done
if lane_enabled conformance; then
  if [[ "$conf_rc" -eq 0 ]]; then
    info "  PASS  Protocol conformance testing (${conf_elapsed}s)"
  else
    error "  FAIL  Protocol conformance testing (${conf_elapsed}s)"
  fi
fi

info "Total wall-clock: ${total_elapsed}s  (sequential estimate: ${sequential_total}s, saved ~${saved}s)"

if [[ "${any_failed}" == "true" ]]; then
  die "One or more quality lanes failed (see above)"
fi

info "All aggregate checks complete"
