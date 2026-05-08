#!/usr/bin/env bash
set -euo pipefail

# Shatter Gauntlet — Broad CLI Coverage
# Exercises shatter's full pipeline against example code, showing output at each stage.
# This is the exhaustive coverage run (flag permutations, all commands, stress cases).
# For the compact demo, see demo/walkthrough.sh.
#
# Usage:
#   ./demo/gauntlet.sh              # Auto-advance: runs all steps continuously
#   ./demo/gauntlet.sh --interactive # Pauses after each step, press Enter to continue
#   ./demo/gauntlet.sh --delay 3    # Auto with N-second delay between steps
#   ./demo/gauntlet.sh --dry-run    # Print commands without executing them

MODE="auto"
DELAY=2
DRY_RUN=false
STEP_TIMEOUT=120  # seconds per step; 0 = no limit
TIMING_DIR=""
TOTAL_STEP_WALL_MS=0

# Use temporary directories so the gauntlet never pollutes repo-local state
# and never contends with Cargo's workspace artifact lock (avoids silent stalls
# when another cargo command is active).
export SHATTER_CACHE_DIR SHATTER_SEEDS_DIR SHATTER_ARTIFACT_DIR RUST_BACKTRACE XDG_CACHE_HOME GOCACHE CARGO_NET_OFFLINE CARGO_TARGET_DIR
SHATTER_CACHE_DIR="$(mktemp -d "${TMPDIR:-/tmp}/shatter-demo-cache.XXXXXX")"
SHATTER_SEEDS_DIR="${SHATTER_CACHE_DIR}/seeds"
# Redirect explore/scan artifact writes (shatter-artifacts/) out of the repo
# so the gauntlet does not leave multi-GB ignored output behind. See
# str-jeen.58.
SHATTER_ARTIFACT_DIR="$(mktemp -d "${TMPDIR:-/tmp}/shatter-demo-artifacts.XXXXXX")"
RUST_BACKTRACE="${RUST_BACKTRACE:-1}"
XDG_CACHE_HOME="${XDG_CACHE_HOME:-$(mktemp -d "${TMPDIR:-/tmp}/shatter-demo-xdg.XXXXXX")}"
GOCACHE="${GOCACHE:-$(mktemp -d "${TMPDIR:-/tmp}/shatter-demo-gocache.XXXXXX")}"
CARGO_NET_OFFLINE="${CARGO_NET_OFFLINE:-true}"
# Isolate Cargo's target directory so the gauntlet doesn't contend with
# concurrent cargo commands (e.g. cargo test) over the shared artifact lock.
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$(mktemp -d "${TMPDIR:-/tmp}/shatter-demo-cargo-target.XXXXXX")}"

# HTML reports are written here; intentionally NOT cleaned up so user can inspect after.
HTML_REPORT_DIR="$(mktemp -d "${TMPDIR:-/tmp}/shatter-gauntlet.XXXXXX")"

# Error tracking: collect failures for a summary at the end.
ERROR_LOG="$(mktemp "${TMPDIR:-/tmp}/shatter-gauntlet-errors.XXXXXX")"
STEP_ERRORS=0
EXAMPLES_ROOT=""
BENCH_MANIFEST_TMP=""

cleanup() { rm -rf "$SHATTER_CACHE_DIR" "$SHATTER_ARTIFACT_DIR" "$ERROR_LOG" "$XDG_CACHE_HOME" "$GOCACHE" "$CARGO_TARGET_DIR" "$EXAMPLES_ROOT" "$BENCH_MANIFEST_TMP" || true; }
trap cleanup EXIT

# Ensure bindgen can find stdbool.h via GCC's include path (avoids requiring libclang-dev)
if command -v gcc &>/dev/null; then
    export BINDGEN_EXTRA_CLANG_ARGS="${BINDGEN_EXTRA_CLANG_ARGS:-} -I$(gcc -print-file-name=include)"
fi

# Color support (disabled if not a terminal)
if [[ -t 1 ]]; then
    BOLD=$'\033[1m'
    DIM=$'\033[2m'
    GREEN=$'\033[32m'
    CYAN=$'\033[36m'
    YELLOW=$'\033[33m'
    RED=$'\033[31m'
    RESET=$'\033[0m'
    # Force color in shatter commands — stdout goes through tee for error
    # capture, which breaks TTY detection in the child process.
    SHATTER_COLOR="always"
else
    BOLD="" DIM="" GREEN="" CYAN="" YELLOW="" RED="" RESET=""
    SHATTER_COLOR="never"
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

example_path() {
    local path="$1"
    if [[ "$path" == examples/* ]]; then
        printf '%s\n' "$EXAMPLES_ROOT/${path#examples/}"
    else
        printf '%s\n' "$path"
    fi
}

echo "${YELLOW}Cloning clean examples checkout...${RESET}"
if ! EXAMPLES_ROOT="$(python3 "$REPO_ROOT/scripts/examples_checkout.py" --fresh)"; then
    echo "${RED}failed to prepare examples checkout${RESET}"
    exit 1
fi
if [[ ! -f "$(example_path "examples/standalone/ts/01-arithmetic.ts")" ]]; then
    echo "${RED}clean examples checkout is missing gauntlet fixtures${RESET}"
    exit 1
fi

EXAMPLES_TS_DIR="$(example_path "examples/standalone/ts")"
EXAMPLES_RUST_SRC_DIR="$(example_path "examples/rust/src")"
EXAMPLES_TS_CONFIG="$(example_path "examples/typescript/.shatter/config.yaml")"
TS_ARITHMETIC_FN="$(example_path "examples/standalone/ts/01-arithmetic.ts:classifyNumber")"
TS_ARITHMETIC_FILE="$(example_path "examples/standalone/ts/01-arithmetic.ts")"
TS_OBJECTS_FN="$(example_path "examples/standalone/ts/03-objects.ts:categorizeUser")"
TS_STRINGS_FN="$(example_path "examples/standalone/ts/02-strings.ts:classifyString")"
TS_MCDC_FILE="$(example_path "examples/standalone/ts/13-mcdc-compound.ts")"

if [[ -n "${SHATTER_BIN:-}" ]]; then
    SHATTER="$SHATTER_BIN"
elif [[ -x "target/debug/shatter" ]]; then
    SHATTER="$(pwd)/target/debug/shatter"
else
    echo "${RED}shatter binary not found.${RESET}"
    echo "Build it first with: cargo build --bin shatter"
    echo "Or run with SHATTER_BIN=/path/to/shatter"
    exit 1
fi

if [[ -n "${SHATTER_RUST_FRONTEND:-}" ]]; then
    SHATTER_RUST_FRONTEND="$SHATTER_RUST_FRONTEND"
elif command -v shatter-rust &>/dev/null; then
    SHATTER_RUST_FRONTEND="$(command -v shatter-rust)"
elif [[ -x "target/debug/shatter-rust" ]]; then
    SHATTER_RUST_FRONTEND="$(pwd)/target/debug/shatter-rust"
elif [[ -x "shatter-rust/target/debug/shatter-rust" ]]; then
    SHATTER_RUST_FRONTEND="$(pwd)/shatter-rust/target/debug/shatter-rust"
else
    echo "${RED}shatter-rust frontend not found.${RESET}"
    echo "Build it first with: cargo build --manifest-path shatter-rust/Cargo.toml"
    exit 1
fi

usage() {
    cat <<EOF
${BOLD}Shatter Gauntlet${RESET} — broad CLI coverage run against example code

${BOLD}USAGE${RESET}
    ./demo/gauntlet.sh [OPTIONS]

${BOLD}OPTIONS${RESET}
    --interactive   Pause after each step (press Enter to continue)
    --auto          (no-op, auto is the default)
    --delay N       Seconds between steps in auto mode (default: 2)
    --step-timeout N  Per-step timeout in seconds (default: 120, 0 = no limit)
    --timing-dir DIR  Persist timing artifacts there and print a 3-line summary per shatter step
    --dry-run       Print commands without executing them
    --help, -h      Show this help

${BOLD}MODES${RESET}
    Auto (default)          Runs continuously with optional delay
    Interactive             Pauses after each step, press Enter to continue
    Dry-run                 Shows what would run, useful before core is built
EOF
    exit 0
}

# Parse args
while [[ $# -gt 0 ]]; do
    case "$1" in
        --auto)    MODE="auto"; shift ;;  # no-op, auto is already the default
        --interactive) MODE="interactive"; shift ;;
        --delay)   DELAY="$2"; shift 2 ;;
        --step-timeout) STEP_TIMEOUT="$2"; shift 2 ;;
        --timing-dir) TIMING_DIR="$2"; shift 2 ;;
        --dry-run) DRY_RUN=true; shift ;;
        --help|-h) usage ;;
        *)         echo "${RED}Unknown option: $1${RESET}"; echo "Run with --help for usage."; exit 1 ;;
    esac
done

banner() {
    local num="$1" total="$2" title="$3" desc="$4"
    echo ""
    echo "${BOLD}${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
    echo "${BOLD}  Step ${num}/${total} · ${title}${RESET}"
    echo "${DIM}  ${desc}${RESET}"
    echo "${BOLD}${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
    echo ""
}

now_ms() {
    if date +%s%3N >/dev/null 2>&1; then
        date +%s%3N
    else
        python3 - <<'PY'
import time
print(int(time.time() * 1000))
PY
    fi
}

format_duration_ms() {
    python3 - "$1" <<'PY'
import sys

duration_ms = int(sys.argv[1])
minutes, remainder = divmod(duration_ms, 60_000)
seconds = remainder / 1000.0

if minutes > 0:
    print(f"{minutes}m {seconds:05.2f}s")
else:
    print(f"{seconds:.2f}s")
PY
}

run_cmd() {
    # Print the command
    echo "${DIM}\$${RESET} ${YELLOW}$*${RESET}"
    echo ""

    local wants_timing=false
    local latest_timing_file=""
    local cmd=("$@")
    # Inject --color flag so termimad renders markdown even though stdout is
    # piped through tee (which hides the real TTY from the child process).
    if [[ ${#cmd[@]} -ge 2 && "${cmd[0]}" == "$SHATTER" ]]; then
        cmd=("$SHATTER" --color "$SHATTER_COLOR" "${cmd[@]:1}")
    fi
    if [[ -n "$TIMING_DIR" && ${#cmd[@]} -ge 2 && "${cmd[0]}" == "$SHATTER" ]]; then
        mkdir -p "$TIMING_DIR"
        cmd=("$SHATTER" --timing summary --timing-format json --timing-output-dir "$TIMING_DIR" "${cmd[@]:1}")
        wants_timing=true
    fi

    if [[ "$DRY_RUN" == true ]]; then
        echo "${DIM}  (dry-run: skipped)${RESET}"
    else
        # Capture combined output (stdout+stderr) while still displaying it.
        # We scan the captured output afterward for error indicators.
        local output_tmp
        output_tmp="$(mktemp)"
        if [[ "$STEP_TIMEOUT" -gt 0 ]]; then
            cmd=(timeout --signal=TERM --kill-after=10 "$STEP_TIMEOUT" "${cmd[@]}")
        fi
        if "${cmd[@]}" </dev/null > >(tee -a "$output_tmp") 2> >(tee -a "$output_tmp" >&2); then
            true
        else
            local rc=$?
            echo ""
            if [[ $rc -eq 124 ]]; then
                echo "${RED}  Step timed out after ${STEP_TIMEOUT}s${RESET}"
                echo "  Step ${CURRENT_STEP}: timed out after ${STEP_TIMEOUT}s" >> "$ERROR_LOG"
            else
                echo "${RED}  Command exited with status ${rc}${RESET}"
                echo "  Step ${CURRENT_STEP}: exit code ${rc}" >> "$ERROR_LOG"
            fi
            STEP_ERRORS=$((STEP_ERRORS + 1))
        fi
        # Wait for tee subprocesses to flush
        wait 2>/dev/null || true
        # Scan for error indicators in the captured output
        local error_pattern='\[error\]|failed to deserialize|panic|SIGSEGV|error: exploration error'
        if grep -qiE "$error_pattern" "$output_tmp" 2>/dev/null; then
            echo "  Step ${CURRENT_STEP}: errors detected:" >> "$ERROR_LOG"
            grep -iE "$error_pattern" "$output_tmp" \
                | sed 's/^/    /' >> "$ERROR_LOG"
            STEP_ERRORS=$((STEP_ERRORS + 1))
        fi
        if [[ "$wants_timing" == true ]]; then
            latest_timing_file="$(find "$TIMING_DIR" -maxdepth 1 -name '*.timing.json' -type f -printf '%T@ %p\n' | sort -nr | head -n1 | cut -d' ' -f2-)"
            if [[ -n "$latest_timing_file" && -f "$latest_timing_file" ]]; then
                python3 - "$latest_timing_file" <<'PY'
import json
import sys

path = sys.argv[1]
with open(path, "r", encoding="utf-8") as fh:
    data = json.load(fh)

phases = data.get("phases", [])
phase_map = {phase["phase_path"]: phase for phase in phases}

total_ms = phase_map.get("cli.command", {}).get("total_ms", float(data.get("duration_ms", 0)))
code_under_test_ms = 0.0
for name, phase in phase_map.items():
    if name.startswith("frontend.remote.execute.invoke_function") or name.startswith("frontend.remote.execute.await_result"):
        code_under_test_ms += float(phase.get("total_ms", 0.0))

shatter_overhead_ms = max(0.0, total_ms - code_under_test_ms)

top_candidates = []
skip_top_phase_names = {
    "cli.command",
    "cli.finalize_command",
    "core.explore_command",
    "core.scan_command",
    "core.run_command",
    "core.export_tests_command",
    "core.revalidate_command",
    "core.stale_command",
    "core.spec_diff_command",
}
for phase in phases:
    name = phase["phase_path"]
    if name in skip_top_phase_names:
        continue
    top_candidates.append((float(phase.get("total_ms", 0.0)), name))
top_candidates.sort(reverse=True)
top_labels = ", ".join(f"{name} {total:.1f}ms" for total, name in top_candidates[:2]) or "no phase data"

print(f"  Timing: total {total_ms:.1f}ms")
print(f"  Timing: shatter {shatter_overhead_ms:.1f}ms | code under test {code_under_test_ms:.1f}ms")
print(f"  Timing: top phases {top_labels}")
PY
            fi
        fi
        rm -f "$output_tmp"
    fi
    echo ""
}

# Track current step number for error reporting
CURRENT_STEP=0

SAMPLE_MANIFEST="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/benchmarks/sample-manifest.json"

load_sample_group() {
    local key="$1"
    python3 - "$SAMPLE_MANIFEST" "$key" <<'PY'
import json
import sys

manifest_path, dotted_key = sys.argv[1], sys.argv[2]
with open(manifest_path, "r", encoding="utf-8") as fh:
    data = json.load(fh)

value = data
for part in dotted_key.split("."):
    value = value[part]

for item in value:
    print(item)
PY
}

pause() {
    if [[ "$MODE" == "interactive" ]]; then
        read -rp "${DIM}[Press Enter to continue]${RESET} "
    else
        sleep "$DELAY"
    fi
    echo ""
}

EXPECTED_NEXT_STEP=1
STEPS_RUN=0

step() {
    local num="$1" total="$2" title="$3" desc="$4"
    shift 4
    if [[ "$num" -ne "$EXPECTED_NEXT_STEP" ]]; then
        echo "${RED}gauntlet step numbering error: got step ${num}, expected ${EXPECTED_NEXT_STEP} (${title})${RESET}" >&2
        exit 2
    fi
    if [[ "$num" -gt "$total" ]]; then
        echo "${RED}gauntlet step numbering error: step ${num} exceeds declared total ${total}${RESET}" >&2
        exit 2
    fi
    EXPECTED_NEXT_STEP=$((EXPECTED_NEXT_STEP + 1))
    STEPS_RUN=$((STEPS_RUN + 1))
    local step_started_ms step_elapsed_ms
    CURRENT_STEP="$num"
    step_started_ms="$(now_ms)"
    banner "$num" "$total" "$title" "$desc"
    run_cmd "$@"
    step_elapsed_ms=$(( $(now_ms) - step_started_ms ))
    TOTAL_STEP_WALL_MS=$((TOTAL_STEP_WALL_MS + step_elapsed_ms))
    echo "${DIM}  Wall time: $(format_duration_ms "$step_elapsed_ms")${RESET}"
    echo ""
    pause
}

# ─── Example targets ──────────────────────────────────────────────────
# Standalone examples: self-contained files with no project dependencies.
# Include one branch-dense "advanced" example in the guided demo. The other
# new mirrored examples stay in scan coverage to keep the gauntlet readable.
mapfile -t EXAMPLES < <(load_sample_group "walkthrough.typescript" | while IFS= read -r sample; do example_path "$sample"; done)
mapfile -t GO_EXAMPLES < <(load_sample_group "walkthrough.go" | while IFS= read -r sample; do example_path "$sample"; done)
mapfile -t RUST_EXAMPLES < <(load_sample_group "walkthrough.rust" | while IFS= read -r sample; do example_path "$sample"; done)

TOTAL=60

# ─── Gauntlet ─────────────────────────────────────────────────────────

echo ""
echo "${BOLD}${GREEN}Shatter Gauntlet${RESET}"
echo "${DIM}Exercising shatter's pipeline against ${#EXAMPLES[@]} TS + ${#GO_EXAMPLES[@]} Go + ${#RUST_EXAMPLES[@]} Rust example functions${RESET}"
echo "${DIM}HTML reports will be written to: ${HTML_REPORT_DIR}/${RESET}"
echo "${DIM}Using clean examples checkout: ${EXAMPLES_ROOT}${RESET}"
if [[ "$DRY_RUN" == true ]]; then
    echo "${YELLOW}(dry-run mode: commands will not be executed)${RESET}"
fi
if [[ -n "$TIMING_DIR" ]]; then
    echo "${DIM}Timing summaries enabled; artifacts will be written to ${TIMING_DIR}${RESET}"
fi

# TypeScript frontend is embedded in the shatter binary — no manual build needed.

# Stage 1: Initialize Project
step 1 $TOTAL "Initialize Project" \
    "Create .shatter/ directory structure with sensible defaults" \
    $SHATTER init

# Stage 2: Analyze
step 2 $TOTAL "Analyze Target Functions" \
    "Discover parameters, types, and branch conditions" \
    $SHATTER explore --analyze-only "${EXAMPLES[@]}"

# Stage 2: Analyze with scope config
step 3 $TOTAL "Analyze with Scope Config" \
    "Load a scope config to control mocking and file inclusion" \
    $SHATTER explore --analyze-only --scope shatter.scope.yaml.example "${EXAMPLES[@]}"

# Stage 3: Explore
step 4 $TOTAL "Generate & Execute Inputs" \
    "Concolic execution: generate inputs to cover all branches" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 -o "$HTML_REPORT_DIR/explore.html" --stdout "${EXAMPLES[@]}"

# Stage 4: Clusters
step 5 $TOTAL "Show Behavior Clusters" \
    "Group executions by branch path into distinct behaviors" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 --show-clusters "${EXAMPLES[@]}"

# Stage 5: Scan standalone TS files
step 6 $TOTAL "Scan Standalone TypeScript" \
    "Scan standalone TypeScript files (no project dependencies needed)" \
    $SHATTER scan -o "$HTML_REPORT_DIR/scan.html" --stdout "$EXAMPLES_TS_DIR"

# Stage 6: Cache behavior maps
step 7 $TOTAL "Explore with Disk Cache" \
    "Persist behavior maps to disk for reuse across runs (SHATTER_CACHE_DIR)" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 "${EXAMPLES[@]}"

# Stage 7: Analyze Go functions
step 8 $TOTAL "Analyze Go Functions" \
    "Discover parameters, types, and branch conditions in Go code" \
    $SHATTER explore --analyze-only "${GO_EXAMPLES[@]}"

# Stage 8: Explore Go functions
step 9 $TOTAL "Explore Go Functions" \
    "Concolic execution on Go: generate inputs to cover all branches" \
    $SHATTER explore --max-iterations 3 --timeout-explore 25 -o "$HTML_REPORT_DIR/explore-go.html" --stdout "${GO_EXAMPLES[@]}"

# Rust frontend is a separate binary. The CLI auto-discovers it from PATH or
# the standard target directories, so just surface which one the gauntlet
# is using instead of invoking Cargo during the run.
echo "${DIM}Using Rust frontend: ${SHATTER_RUST_FRONTEND}${RESET}"

# Stage 9: Analyze Rust functions
step 10 $TOTAL "Analyze Rust Functions" \
    "Discover parameters, types, and branch conditions in Rust code" \
    $SHATTER explore --analyze-only "${RUST_EXAMPLES[@]}"

# Stage 10: Explore Rust functions
step 11 $TOTAL "Explore Rust Functions" \
    "Concolic execution on Rust: generate inputs to cover all branches" \
    $SHATTER explore --max-iterations 3 --timeout-explore 25 -o "$HTML_REPORT_DIR/explore-rust.html" --stdout "${RUST_EXAMPLES[@]}"

# Stage 11: Scan Rust examples (project with deps)
step 12 $TOTAL "Scan Rust Examples" \
    "Preview a dependency-ordered Rust scan plan on a representative sample" \
    $SHATTER scan --core-sample 3 --dry-run "$EXAMPLES_RUST_SRC_DIR"

# Stage 13: Run (full pipeline, analyze only)
step 13 $TOTAL "Run: Analyze Only" \
    "Discover, analyze, and report on all files in the standalone TS directory" \
    $SHATTER run --analyze-only "$EXAMPLES_TS_DIR"

# Stage 14: Run (full pipeline with exploration)
step 14 $TOTAL "Run: Full Pipeline" \
    "Discover, analyze, explore, and generate a full report" \
    $SHATTER run --max-iterations 10 --timeout 60 "$EXAMPLES_TS_DIR"

# Stage 15: Log level verbosity (debug)
step 15 $TOTAL "Verbose Output with Debug Log Level" \
    "Show detailed progress output using --log-level debug" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 --log-level debug "${EXAMPLES[0]}"

# Stage 16: Request timeout
step 16 $TOTAL "Request Timeout" \
    "Set a per-request timeout to bound frontend communication" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 --request-timeout 15 "${EXAMPLES[@]}"

# Stage 17: User-provided inputs via config
step 17 $TOTAL "User-Provided Inputs via Config" \
    "Load candidate inputs from a .shatter config directory" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 --config "$EXAMPLES_TS_CONFIG" \
    "${EXAMPLES[0]}"

# Stage 18: Performance stats
step 18 $TOTAL "Performance Stats" \
    "Show gauntlet timing summaries using structured timing artifacts" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 "${EXAMPLES[@]}"

# Stage 19: Parallel scan with worker pool
step 19 $TOTAL "Parallel Scan" \
    "Scan with multiple worker processes for faster exploration" \
    $SHATTER scan --parallelism 2 --timeout-per-fn 30 "$EXAMPLES_TS_DIR"

# Stage 20: Parallel explore with --workers
step 20 $TOTAL "Parallel Explore" \
    "Explore multiple functions in parallel using --workers (limits concurrency)" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 --workers 2 "${EXAMPLES[@]}"

# Stage 22: Execution timeout
step 21 $TOTAL "Execution Timeout" \
    "Configure per-execution timeout passed to frontends" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 --exec-timeout 5 --build-timeout 20 "${EXAMPLES[0]}"

# Stage 22: Go execution timeout
step 22 $TOTAL "Go Execution Timeout" \
    "Configurable timeouts also apply to the Go frontend" \
    $SHATTER explore --max-iterations 3 --timeout-explore 25 --exec-timeout 8 "${GO_EXAMPLES[0]}"

# Stage 22: Scan with total timeout
step 23 $TOTAL "Scan Total Timeout" \
    "Bound overall scan wall-clock time with --timeout-total" \
    $SHATTER scan --timeout-total 120 --timeout-per-fn 30 "$EXAMPLES_TS_DIR"

# Stage 23: Memory limit
step 24 $TOTAL "Memory Limit" \
    "Cap frontend memory usage (sets --max-old-space-size for TS, GOMEMLIMIT for Go)" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 --memory-limit 512 "${EXAMPLES[0]}"

# Stage 24: Behavioral specification (markdown)
step 25 $TOTAL "Behavioral Specification (Markdown)" \
    "Generate a behavioral spec with equivalence classes, pre/postconditions" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 --spec "${EXAMPLES[0]}"

# Stage 25: Behavioral specification (JSON)
step 26 $TOTAL "Behavioral Specification (JSON)" \
    "Machine-readable JSON spec for tooling integration" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 --spec-json "${EXAMPLES[0]}"

# Stage 26: Spec diff
# Generate specs from v1 and v2 fixture variants of classifyNumber and diff them.
# v2 adds a "large" threshold, so the diff shows added/changed behaviors.
step 27 $TOTAL "Specification Diff" \
    "Compare behavioral specs from two versions of classifyNumber to detect regressions" \
    bash -c "$SHATTER explore --max-iterations 20 --timeout-explore 15 --quiet --spec-json 'demo/fixtures/arithmetic-v1.ts:classifyNumber' > /tmp/shatter-spec-old.json && $SHATTER explore --max-iterations 20 --timeout-explore 15 --quiet --spec-json 'demo/fixtures/arithmetic-v2.ts:classifyNumber' > /tmp/shatter-spec-new.json && { $SHATTER spec-diff /tmp/shatter-spec-old.json /tmp/shatter-spec-new.json; true; }"

# Stage 27: Cross-language compare
# Reuses the v1 and v2 spec files from step 27 to demonstrate cross-language
# behavioral comparison. Compares by input/output behavior, ignoring branch paths.
step 28 $TOTAL "Cross-Language Compare" \
    "Compare two specs by input/output behavior (ignoring language-specific branch paths)" \
    bash -c "$SHATTER compare /tmp/shatter-spec-old.json /tmp/shatter-spec-new.json; true"

# Stage 28: Explore without boundary values
step 29 $TOTAL "Explore Without Boundary Values" \
    "Disable built-in boundary value seeding with --no-boundary-values" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 --no-boundary-values "${EXAMPLES[0]}"

# Stage 29: Markdown scan report
step 30 $TOTAL "Markdown Scan Report" \
    "Generate a human-readable markdown report alongside JSON" \
    $SHATTER scan -o /tmp/shatter-scan-report.md "$EXAMPLES_TS_DIR"

# Stage 30: Scan dry-run
step 31 $TOTAL "Scan Dry Run" \
    "Preview which files would be scanned without executing" \
    $SHATTER scan --dry-run --language typescript "$EXAMPLES_TS_DIR"

# Stage 31: Incremental scan (--since)
step 32 $TOTAL "Incremental Scan (--since)" \
    "Scan only files changed since the initial examples commit" \
    bash -c "$SHATTER scan --since HEAD~1 '"$EXAMPLES_TS_DIR"'; echo '(exit code 1 is acceptable here: selected file may already be fresh against prior scan artifacts)'"

# Stage 32: Incremental scan (--changed)
step 33 $TOTAL "Incremental Scan (--changed)" \
    "Scan only files with uncommitted changes (expect: no files in clean clone)" \
    $SHATTER scan --changed "$EXAMPLES_TS_DIR"

# Stage 33: Invariant detection
step 34 $TOTAL "Invariant Detection" \
    "Detect Daikon-style invariants over explored executions" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 --invariants "${EXAMPLES[0]}"

# Stage 34: Setup + generators via config
step 35 $TOTAL "Setup + Generators via Config" \
    "Explore with setup/teardown lifecycle and custom type generators from .shatter/config.yaml" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 --config "$EXAMPLES_TS_CONFIG" \
    "$TS_OBJECTS_FN"

# Stage 35: Setup + generators with debug logging
step 36 $TOTAL "Setup + Generators (Debug)" \
    "Show setup/teardown and generator lifecycle with --log-level debug" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 --config "$EXAMPLES_TS_CONFIG" \
    --log-level debug "$TS_OBJECTS_FN"

# Stage 36: File-level explore (all exported functions)
step 37 $TOTAL "File-Level Explore" \
    "Explore all exported functions in a file by passing just the file path" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 "$TS_ARITHMETIC_FILE"

# Stage 37: Concolic exploration (Z3-backed)
step 38 $TOTAL "Concolic CLI Preview (Z3)" \
    "Preview the current Z3-backed CLI path on a numeric example" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 --concolic -o "$HTML_REPORT_DIR/explore-concolic.html" --stdout "${EXAMPLES[0]}"

# Stage 38: Concolic exploration of string functions (Z3 string ops)
step 39 $TOTAL "Concolic String CLI Preview (Z3)" \
    "Preview the current Z3-backed CLI path on string-method guards" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 --concolic "$TS_STRINGS_FN"

# Stage 39: MC/DC coverage analysis
step 40 $TOTAL "MC/DC Coverage Analysis" \
    "Modified Condition/Decision Coverage: independence pairs, short-circuit masking, and coverage % across AND/OR/three-way compound conditions" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 --mcdc "$TS_MCDC_FILE"

# Stage 40: Spec output to file (--output)
step 41 $TOTAL "Spec Output to File" \
    "Write a spec bundle to a JSON file with --output (includes fingerprints)" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 --output /tmp/shatter-spec.json "${EXAMPLES[0]}"

# Stage 41: Incremental re-run (skips fresh functions)
step 42 $TOTAL "Incremental Re-run" \
    "Re-run with --output against existing spec — unchanged functions are skipped" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 --output /tmp/shatter-spec.json "${EXAMPLES[0]}"

# Stage 42: Dry-run mode
step 43 $TOTAL "Dry-Run Mode" \
    "Use --dry-run to preview which functions would be re-explored without actually exploring" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 --output /tmp/shatter-spec.json --dry-run "${EXAMPLES[0]}"

# Stage 43: Clean re-exploration
step 44 $TOTAL "Clean Re-exploration" \
    "Use --clean to force full re-exploration, ignoring the existing spec" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 --output /tmp/shatter-spec.json --clean "${EXAMPLES[0]}"

# Stage 44: Stale command
# The spec from step 40 only explored classifyNumber. The file also exports
# compareMagnitudes, so `stale` correctly reports it as stale. Exit code 1
# means "some functions are stale or removed" — this is informational, not a failure.
step 45 $TOTAL "Stale Check" \
    "Check staleness relative to spec from step 40 (exit 1 = stale found, expected here)" \
    bash -c "$SHATTER stale '"$TS_ARITHMETIC_FILE"' /tmp/shatter-spec.json; echo '(exit code 1 is expected: compareMagnitudes was not in the spec from step 40)'"

# Stage 45: Revalidate command
# Re-execute cached behaviors to check for drift/regressions. Uses cache
# populated by earlier explore steps. Exit code 0 = no regressions found.
step 46 $TOTAL "Revalidate" \
    "Revalidate cached behaviors for the arithmetic example" \
    bash -c "$SHATTER revalidate '"$TS_ARITHMETIC_FILE"'; echo '(exit code 1 is acceptable here: cached behaviors may replay as flaky in demo mode)'"

# Stage 46: Multi-level setup/teardown
step 47 $TOTAL "Multi-Level Setup/Teardown" \
    "Explore with session + file level setup/teardown from .shatter/config.yaml" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 --config "$EXAMPLES_TS_CONFIG" \
    --setup-timeout 30 \
    "$TS_ARITHMETIC_FN"

# Stage 47: Setup with --fail-on-setup-error
step 48 $TOTAL "Setup Fail-on-Error" \
    "Use --fail-on-setup-error to abort immediately on setup failures" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 --config "$EXAMPLES_TS_CONFIG" \
    --setup-timeout 10 --fail-on-setup-error \
    "$TS_ARITHMETIC_FN"

# Stage 48: Observe command — run observation stage, write ObserveStageOutput JSON
step 49 $TOTAL "Observe Stage" \
    "Run observation stage only for classifyNumber, write to temp file" \
    $SHATTER observe --output /tmp/shatter-observe.json \
    "$TS_ARITHMETIC_FN"

# Stage 49: Analyze observe output — offline analysis, no frontend needed
step 50 $TOTAL "Analyze Observe Output" \
    "Read observation output and run offline analysis stage" \
    $SHATTER analyze /tmp/shatter-observe.json

# Stage 50: Solve uncovered branches — offline Z3 constraint solving
step 51 $TOTAL "Solve Uncovered Branches" \
    "Run Z3 constraint solver on observation output to find inputs for uncovered branches" \
    $SHATTER solve /tmp/shatter-observe.json

# Stage 51: Specify from observation — build FunctionSpec markdown
step 52 $TOTAL "Specify from Observation" \
    "Build FunctionSpec markdown from observation output" \
    $SHATTER specify /tmp/shatter-observe.json

# Stage 52: Specify YAML — build FunctionSpec with invariant property descriptions
step 53 $TOTAL "Specify from Observation (YAML)" \
    "Build FunctionSpec as YAML with inferred invariant property descriptions" \
    $SHATTER specify --yaml --invariants /tmp/shatter-observe.json

# Stage 53: HTML explore report
step 54 $TOTAL "HTML Explore Report" \
    "Generate a self-contained HTML report for exploration results" \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 -o "$HTML_REPORT_DIR/explore-html.html" --stdout "${EXAMPLES[0]}"

# Stage 54: HTML scan report
step 55 $TOTAL "HTML Scan Report" \
    "Generate a self-contained HTML scan report alongside JSON" \
    $SHATTER scan -o "$HTML_REPORT_DIR/scan-html.html" --stdout "$EXAMPLES_TS_DIR"

# Stage 55: Side-effect capture
step 56 $TOTAL "Explore with Side-Effect Capture" \
    "Opt in to rich side-effect recording (console output, global state changes). Disabled by default for throughput." \
    $SHATTER explore --max-iterations 20 --timeout-explore 15 --capture-side-effects "${EXAMPLES[0]}"

# Stage 56: Properties export
step 57 $TOTAL "Properties Export" \
    "Discover and export behavioral properties and invariants as a YAML spec." \
    $SHATTER properties "${EXAMPLES[0]}"

# Stage 57: Nondeterminism review (non-interactive: reads from cache populated above).
# The command runs in non-interactive mode when stdin is not a terminal (as in
# the gauntlet). With no candidates in the cache it prints a diagnostic
# message and exits 0, which is the expected outcome after the standalone scan.
step 58 $TOTAL "Nondeterminism Review" \
    "Review nondeterminism candidates from the most recent scan (non-interactive: no stdin)" \
    bash -c "$SHATTER nondeterminism review --cache-dir '$SHATTER_CACHE_DIR' </dev/null; echo '(exit 0 expected: no nondeterminism candidates in standalone arithmetic scan)'"

# Stage 58: Benchmark run (smoke tier, minimal)
# Rewrite the bench manifest so its `examples/...` targets resolve under the
# fresh examples checkout used by the rest of the gauntlet (str-02ws). Without
# this, every smoke-tier scenario fails with FileNotFound while bench still
# exits 0.
BENCH_MANIFEST_TMP="$(mktemp "${TMPDIR:-/tmp}/shatter-bench-manifest.XXXXXX.json")"
python3 - "$SAMPLE_MANIFEST" "$EXAMPLES_ROOT" "$BENCH_MANIFEST_TMP" <<'PY'
import json
import sys

src, examples_root, dst = sys.argv[1], sys.argv[2], sys.argv[3]
with open(src, "r", encoding="utf-8") as fh:
    data = json.load(fh)

def rewrite(value):
    if isinstance(value, str) and value.startswith("examples/"):
        return f"{examples_root}/{value[len('examples/'):]}"
    if isinstance(value, list):
        return [rewrite(v) for v in value]
    if isinstance(value, dict):
        return {k: rewrite(v) for k, v in value.items()}
    return value

with open(dst, "w", encoding="utf-8") as fh:
    json.dump(rewrite(data), fh, indent=2)
PY
step 59 $TOTAL "Benchmark Run (Smoke)" \
    "Run the benchmark harness on the smoke tier with 1 repeat, 0 warmups." \
    $SHATTER bench --manifest "$BENCH_MANIFEST_TMP" --tier smoke --repeats 1 --warmups 0

# Stage 59: Cache clear
step 60 $TOTAL "Cache Clear" \
    "Clear all on-disk caches (analysis + results). Reports file count and bytes freed." \
    $SHATTER cache clear


# ─── Step inventory check ────────────────────────────────────────────
if [[ "$STEPS_RUN" -ne "$TOTAL" ]]; then
    echo "${RED}gauntlet step inventory mismatch: ran ${STEPS_RUN} step(s), declared total ${TOTAL}${RESET}" >&2
    echo "  Step inventory: ran ${STEPS_RUN}, declared ${TOTAL}" >> "$ERROR_LOG"
    STEP_ERRORS=$((STEP_ERRORS + 1))
fi

# ─── HTML Report Summary ──────────────────────────────────────────────
echo ""
echo "${BOLD}${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
echo "${BOLD}  Gauntlet wall time: $(format_duration_ms "$TOTAL_STEP_WALL_MS")${RESET}"
echo "${BOLD}${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
echo ""

echo "${BOLD}${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
echo "${BOLD}  HTML reports written to: ${HTML_REPORT_DIR}/${RESET}"
if ls "$HTML_REPORT_DIR"/*.html &>/dev/null 2>&1; then
    for f in "$HTML_REPORT_DIR"/*.html; do
        echo "${DIM}    $(basename "$f")${RESET}"
    done
else
    echo "${DIM}  (no HTML files found — dry-run mode?)${RESET}"
fi
echo "${BOLD}${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"

# ─── Error Summary ────────────────────────────────────────────────────
if [[ -s "$ERROR_LOG" ]]; then
    echo ""
    echo "${BOLD}${RED}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
    echo "${BOLD}${RED}  ERROR SUMMARY (${STEP_ERRORS} issue(s))${RESET}"
    echo "${BOLD}${RED}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
    cat "$ERROR_LOG"
    echo ""
    echo "${BOLD}${GREEN}Gauntlet complete with errors.${RESET}"
    exit 1
else
    echo "${BOLD}${GREEN}Gauntlet complete. All steps passed.${RESET}"
fi
