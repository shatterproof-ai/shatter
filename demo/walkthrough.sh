#!/usr/bin/env bash
set -euo pipefail

# Shatter Walkthrough / Demo
# Exercises shatter's full pipeline against example code, showing output at each stage.
#
# Usage:
#   ./demo/walkthrough.sh              # Auto-advance: runs all steps continuously
#   ./demo/walkthrough.sh --interactive # Pauses after each step, press Enter to continue
#   ./demo/walkthrough.sh --delay 3    # Auto with N-second delay between steps
#   ./demo/walkthrough.sh --dry-run    # Print commands without executing them

MODE="auto"
DELAY=2
DRY_RUN=false
STEP_TIMEOUT=120  # seconds per step; 0 = no limit
TIMING_DIR=""

# Use temporary directories so the walkthrough never pollutes repo-local state
# and never contends with Cargo's workspace artifact lock (avoids silent stalls
# when another cargo command is active).
export SHATTER_CACHE_DIR SHATTER_SEEDS_DIR RUST_BACKTRACE XDG_CACHE_HOME GOCACHE CARGO_NET_OFFLINE CARGO_TARGET_DIR
SHATTER_CACHE_DIR="$(mktemp -d "${TMPDIR:-/tmp}/shatter-demo-cache.XXXXXX")"
SHATTER_SEEDS_DIR="${SHATTER_CACHE_DIR}/seeds"
RUST_BACKTRACE="${RUST_BACKTRACE:-1}"
XDG_CACHE_HOME="${XDG_CACHE_HOME:-$(mktemp -d "${TMPDIR:-/tmp}/shatter-demo-xdg.XXXXXX")}"
GOCACHE="${GOCACHE:-$(mktemp -d "${TMPDIR:-/tmp}/shatter-demo-gocache.XXXXXX")}"
CARGO_NET_OFFLINE="${CARGO_NET_OFFLINE:-true}"
# Isolate Cargo's target directory so the walkthrough doesn't contend with
# concurrent cargo commands (e.g. cargo test) over the shared artifact lock.
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$(mktemp -d "${TMPDIR:-/tmp}/shatter-demo-cargo-target.XXXXXX")}"

# Error tracking: collect failures for a summary at the end.
ERROR_LOG="$(mktemp "${TMPDIR:-/tmp}/shatter-walkthrough-errors.XXXXXX")"
STEP_ERRORS=0

cleanup() { rm -rf "$SHATTER_CACHE_DIR" "$ERROR_LOG" "$XDG_CACHE_HOME" "$GOCACHE" "$CARGO_TARGET_DIR" || true; }
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
else
    BOLD="" DIM="" GREEN="" CYAN="" YELLOW="" RED="" RESET=""
fi

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

if command -v shatter-rust &>/dev/null; then
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
${BOLD}Shatter Walkthrough${RESET} — exercise shatter's pipeline against example code

${BOLD}USAGE${RESET}
    ./demo/walkthrough.sh [OPTIONS]

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

run_cmd() {
    # Print the command
    echo "${DIM}\$${RESET} ${YELLOW}$*${RESET}"
    echo ""

    local wants_timing=false
    local latest_timing_file=""
    local cmd=("$@")
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

pause() {
    if [[ "$MODE" == "interactive" ]]; then
        read -rp "${DIM}[Press Enter to continue]${RESET} "
    else
        sleep "$DELAY"
    fi
    echo ""
}

step() {
    local num="$1" total="$2" title="$3" desc="$4"
    shift 4
    CURRENT_STEP="$num"
    banner "$num" "$total" "$title" "$desc"
    run_cmd "$@"
    pause
}

# ─── Example targets ──────────────────────────────────────────────────
# Standalone examples: self-contained files with no project dependencies.
# Include one branch-dense "advanced" example in the guided demo. The other
# new mirrored examples stay in scan coverage to keep the walkthrough readable.

EXAMPLES=(
    "examples/standalone/ts/01-arithmetic.ts:classifyNumber"
    "examples/standalone/ts/02-strings.ts:classifyString"
    "examples/standalone/ts/03-objects.ts:categorizeUser"
    "examples/standalone/ts/04-errors.ts:safeDivide"
    "examples/standalone/ts/18-accept-language.ts:negotiateLanguage"
)

GO_EXAMPLES=(
    "examples/standalone/go/01-arithmetic.go:ClassifyNumber"
    "examples/standalone/go/02-strings.go:ClassifyString"
    "examples/standalone/go/04-errors.go:SafeDivide"
    "examples/standalone/go/18-accept-language.go:NegotiateLanguage"
)

RUST_EXAMPLES=(
    "examples/standalone/rust/01_arithmetic.rs:classify_number"
    "examples/standalone/rust/02_strings.rs:classify_string"
    "examples/standalone/rust/04_errors.rs:safe_divide"
    "examples/standalone/rust/18_accept_language.rs:negotiate_language"
)

TOTAL=44

# ─── Walkthrough ──────────────────────────────────────────────────────

echo ""
echo "${BOLD}${GREEN}Shatter Walkthrough${RESET}"
echo "${DIM}Exercising shatter's pipeline against ${#EXAMPLES[@]} TS + ${#GO_EXAMPLES[@]} Go + ${#RUST_EXAMPLES[@]} Rust example functions${RESET}"
if [[ "$DRY_RUN" == true ]]; then
    echo "${YELLOW}(dry-run mode: commands will not be executed)${RESET}"
fi
if [[ -n "$TIMING_DIR" ]]; then
    echo "${DIM}Timing summaries enabled; artifacts will be written to ${TIMING_DIR}${RESET}"
fi

# TypeScript frontend is embedded in the shatter binary — no manual build needed.

# Stage 1: Analyze
step 1 $TOTAL "Analyze Target Functions" \
    "Discover parameters, types, and branch conditions" \
    $SHATTER explore --analyze-only "${EXAMPLES[@]}"

# Stage 2: Analyze with scope config
step 2 $TOTAL "Analyze with Scope Config" \
    "Load a scope config to control mocking and file inclusion" \
    $SHATTER explore --analyze-only --scope shatter.scope.yaml.example "${EXAMPLES[@]}"

# Stage 3: Explore
step 3 $TOTAL "Generate & Execute Inputs" \
    "Concolic execution: generate inputs to cover all branches" \
    $SHATTER explore "${EXAMPLES[@]}"

# Stage 4: Clusters
step 4 $TOTAL "Show Behavior Clusters" \
    "Group executions by branch path into distinct behaviors" \
    $SHATTER explore --show-clusters "${EXAMPLES[@]}"

# Stage 5: Scan standalone TS files
step 5 $TOTAL "Scan Standalone TypeScript" \
    "Scan standalone TypeScript files (no project dependencies needed)" \
    $SHATTER scan examples/standalone/ts

# Stage 6: Cache behavior maps
step 6 $TOTAL "Explore with Disk Cache" \
    "Persist behavior maps to disk for reuse across runs (SHATTER_CACHE_DIR)" \
    $SHATTER explore "${EXAMPLES[@]}"

# Stage 7: Analyze Go functions
step 7 $TOTAL "Analyze Go Functions" \
    "Discover parameters, types, and branch conditions in Go code" \
    $SHATTER explore --analyze-only "${GO_EXAMPLES[@]}"

# Stage 8: Explore Go functions
step 8 $TOTAL "Explore Go Functions" \
    "Concolic execution on Go: generate inputs to cover all branches" \
    $SHATTER explore --max-iterations 3 --timeout-explore 25 "${GO_EXAMPLES[@]}"

# Rust frontend is a separate binary. The CLI auto-discovers it from PATH or
# the standard target directories, so just surface which one the walkthrough
# is using instead of invoking Cargo during the demo.
echo "${DIM}Using Rust frontend: ${SHATTER_RUST_FRONTEND}${RESET}"

# Stage 9: Analyze Rust functions
step 9 $TOTAL "Analyze Rust Functions" \
    "Discover parameters, types, and branch conditions in Rust code" \
    $SHATTER explore --analyze-only "${RUST_EXAMPLES[@]}"

# Stage 10: Explore Rust functions
step 10 $TOTAL "Explore Rust Functions" \
    "Concolic execution on Rust: generate inputs to cover all branches" \
    $SHATTER explore --max-iterations 3 --timeout-explore 25 "${RUST_EXAMPLES[@]}"

# Stage 11: Scan Rust examples (project with deps)
step 11 $TOTAL "Scan Rust Examples" \
    "Preview a dependency-ordered Rust scan plan on a representative sample" \
    $SHATTER scan --core-sample 3 --dry-run examples/rust/src

# Stage 12: Export tests
step 12 $TOTAL "Export Generated Tests" \
    "Generate Jest test files from explored behavior maps" \
    $SHATTER export-tests --framework jest --module-path "./src/01-arithmetic" \
    "examples/standalone/ts/01-arithmetic.ts:classifyNumber"

# Stage 13: Run (full pipeline, analyze only)
step 13 $TOTAL "Run: Analyze Only" \
    "Discover, analyze, and report on all files in the standalone TS directory" \
    $SHATTER run --analyze-only examples/standalone/ts

# Stage 14: Run (full pipeline with exploration)
step 14 $TOTAL "Run: Full Pipeline" \
    "Discover, analyze, explore, and generate a full report" \
    $SHATTER run --max-iterations 10 --timeout 60 examples/standalone/ts

# Stage 15: Log level verbosity (debug)
step 15 $TOTAL "Verbose Output with Debug Log Level" \
    "Show detailed progress output using --log-level debug" \
    $SHATTER explore --log-level debug "${EXAMPLES[0]}"

# Stage 16: Request timeout
step 16 $TOTAL "Request Timeout" \
    "Set a per-request timeout to bound frontend communication" \
    $SHATTER explore --request-timeout 15 "${EXAMPLES[@]}"

# Stage 17: User-provided inputs via config
step 17 $TOTAL "User-Provided Inputs via Config" \
    "Load candidate inputs from a .shatter config directory" \
    $SHATTER explore --config examples/typescript/.shatter/config.yaml \
    "${EXAMPLES[0]}"

# Stage 18: Performance stats
step 18 $TOTAL "Performance Stats" \
    "Show walkthrough timing summaries using structured timing artifacts" \
    $SHATTER explore "${EXAMPLES[@]}"

# Stage 19: Parallel scan with worker pool
step 19 $TOTAL "Parallel Scan" \
    "Scan with multiple worker processes for faster exploration" \
    $SHATTER scan --parallelism 2 --timeout-per-fn 30 examples/standalone/ts

# Stage 20: Execution timeout
step 20 $TOTAL "Execution Timeout" \
    "Configure per-execution timeout passed to frontends" \
    $SHATTER explore --exec-timeout 5 --build-timeout 20 "${EXAMPLES[0]}"

# Stage 21: Go execution timeout
step 21 $TOTAL "Go Execution Timeout" \
    "Configurable timeouts also apply to the Go frontend" \
    $SHATTER explore --exec-timeout 8 "${GO_EXAMPLES[0]}"

# Stage 22: Scan with total timeout
step 22 $TOTAL "Scan Total Timeout" \
    "Bound overall scan wall-clock time with --timeout-total" \
    $SHATTER scan --timeout-total 120 --timeout-per-fn 30 examples/standalone/ts

# Stage 23: Memory limit
step 23 $TOTAL "Memory Limit" \
    "Cap frontend memory usage (sets --max-old-space-size for TS, GOMEMLIMIT for Go)" \
    $SHATTER explore --memory-limit 512 "${EXAMPLES[0]}"

# Stage 24: Behavioral specification (markdown)
step 24 $TOTAL "Behavioral Specification (Markdown)" \
    "Generate a behavioral spec with equivalence classes, pre/postconditions" \
    $SHATTER explore --spec "${EXAMPLES[0]}"

# Stage 25: Behavioral specification (JSON)
step 25 $TOTAL "Behavioral Specification (JSON)" \
    "Machine-readable JSON spec for tooling integration" \
    $SHATTER explore --spec-json "${EXAMPLES[0]}"

# Stage 26: Spec diff
# Generate specs from v1 and v2 fixture variants of classifyNumber and diff them.
# v2 adds a "large" threshold, so the diff shows added/changed behaviors.
step 26 $TOTAL "Specification Diff" \
    "Compare behavioral specs from two versions of classifyNumber to detect regressions" \
    bash -c "$SHATTER explore --quiet --spec-json 'demo/fixtures/arithmetic-v1.ts:classifyNumber' > /tmp/shatter-spec-old.json && $SHATTER explore --quiet --spec-json 'demo/fixtures/arithmetic-v2.ts:classifyNumber' > /tmp/shatter-spec-new.json && { $SHATTER spec-diff /tmp/shatter-spec-old.json /tmp/shatter-spec-new.json; true; }"

# Stage 27: Explore without boundary values
step 27 $TOTAL "Explore Without Boundary Values" \
    "Disable built-in boundary value seeding with --no-boundary-values" \
    $SHATTER explore --no-boundary-values "${EXAMPLES[0]}"

# Stage 28: Emit tests from scan
step 28 $TOTAL "Emit Tests from Scan" \
    "Generate Jest test files from behavior maps discovered during scan" \
    $SHATTER scan --emit-tests jest --output /tmp/shatter-demo-tests \
    examples/standalone/ts

# Stage 29: Markdown scan report
step 29 $TOTAL "Markdown Scan Report" \
    "Generate a human-readable markdown report alongside JSON" \
    $SHATTER scan --format=markdown examples/standalone/ts

# Stage 30: Scan dry-run
step 30 $TOTAL "Scan Dry Run" \
    "Preview which files would be scanned without executing" \
    $SHATTER scan --dry-run --language typescript examples/standalone/ts

# Stage 31: Invariant detection
step 31 $TOTAL "Invariant Detection" \
    "Detect Daikon-style invariants over explored executions" \
    $SHATTER explore --invariants "${EXAMPLES[0]}"

# Stage 32: Setup + generators via config
step 32 $TOTAL "Setup + Generators via Config" \
    "Explore with setup/teardown lifecycle and custom type generators from .shatter/config.yaml" \
    $SHATTER explore --config examples/typescript/.shatter/config.yaml \
    "examples/standalone/ts/03-objects.ts:categorizeUser"

# Stage 33: Setup + generators with debug logging
step 33 $TOTAL "Setup + Generators (Debug)" \
    "Show setup/teardown and generator lifecycle with --log-level debug" \
    $SHATTER explore --config examples/typescript/.shatter/config.yaml \
    --log-level debug "examples/standalone/ts/03-objects.ts:categorizeUser"

# Stage 34: File-level explore (all exported functions)
step 34 $TOTAL "File-Level Explore" \
    "Explore all exported functions in a file by passing just the file path" \
    $SHATTER explore examples/standalone/ts/01-arithmetic.ts

# Stage 35: Concolic exploration (Z3-backed)
step 35 $TOTAL "Concolic CLI Preview (Z3)" \
    "Preview the current Z3-backed CLI path on a numeric example" \
    $SHATTER explore --concolic "${EXAMPLES[0]}"

# Stage 36: Concolic exploration of string functions (Z3 string ops)
step 36 $TOTAL "Concolic String CLI Preview (Z3)" \
    "Preview the current Z3-backed CLI path on string-method guards" \
    $SHATTER explore --concolic "examples/standalone/ts/02-strings.ts:classifyString"

# Stage 37: Spec output to file (--output)
step 37 $TOTAL "Spec Output to File" \
    "Write a spec bundle to a JSON file with --output (includes fingerprints)" \
    $SHATTER explore --output /tmp/shatter-spec.json "${EXAMPLES[0]}"

# Stage 38: Incremental re-run (skips fresh functions)
step 38 $TOTAL "Incremental Re-run" \
    "Re-run with --output against existing spec — unchanged functions are skipped" \
    $SHATTER explore --output /tmp/shatter-spec.json "${EXAMPLES[0]}"

# Stage 39: Dry-run mode
step 39 $TOTAL "Dry-Run Mode" \
    "Use --dry-run to preview which functions would be re-explored without actually exploring" \
    $SHATTER explore --output /tmp/shatter-spec.json --dry-run "${EXAMPLES[0]}"

# Stage 40: Clean re-exploration
step 40 $TOTAL "Clean Re-exploration" \
    "Use --clean to force full re-exploration, ignoring the existing spec" \
    $SHATTER explore --output /tmp/shatter-spec.json --clean "${EXAMPLES[0]}"

# Stage 41: Stale command
# The spec from step 37 only explored classifyNumber. The file also exports
# compareMagnitudes, so `stale` correctly reports it as stale. Exit code 1
# means "some functions are stale or removed" — this is informational, not a failure.
step 41 $TOTAL "Stale Check" \
    "Check staleness relative to spec from step 37 (exit 1 = stale found, expected here)" \
    bash -c "$SHATTER stale 'examples/standalone/ts/01-arithmetic.ts' /tmp/shatter-spec.json; echo '(exit code 1 is expected: compareMagnitudes was not in the spec from step 37)'"

# Stage 42: Revalidate command
# Re-execute cached behaviors to check for drift/regressions. Uses cache
# populated by earlier explore steps. Exit code 0 = no regressions found.
step 42 $TOTAL "Revalidate" \
    "Revalidate cached behaviors for the arithmetic example" \
    $SHATTER revalidate 'examples/standalone/ts/01-arithmetic.ts'

# Stage 43: Multi-level setup/teardown
step 43 $TOTAL "Multi-Level Setup/Teardown" \
    "Explore with session + file level setup/teardown from .shatter/config.yaml" \
    $SHATTER explore --config examples/typescript/.shatter/config.yaml \
    --setup-timeout 30 \
    "examples/standalone/ts/01-arithmetic.ts:classifyNumber"

# Stage 44: Setup with --fail-on-setup-error
step 44 $TOTAL "Setup Fail-on-Error" \
    "Use --fail-on-setup-error to abort immediately on setup failures" \
    $SHATTER explore --config examples/typescript/.shatter/config.yaml \
    --setup-timeout 10 --fail-on-setup-error \
    "examples/standalone/ts/01-arithmetic.ts:classifyNumber"

# ─── Error Summary ────────────────────────────────────────────────────
if [[ -s "$ERROR_LOG" ]]; then
    echo ""
    echo "${BOLD}${RED}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
    echo "${BOLD}${RED}  ERROR SUMMARY (${STEP_ERRORS} issue(s))${RESET}"
    echo "${BOLD}${RED}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
    cat "$ERROR_LOG"
    echo ""
    echo "${BOLD}${GREEN}Walkthrough complete with errors.${RESET}"
    exit 1
else
    echo "${BOLD}${GREEN}Walkthrough complete. All steps passed.${RESET}"
fi
