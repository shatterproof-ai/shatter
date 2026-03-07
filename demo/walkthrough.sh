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
SHATTER="cargo run --quiet --bin shatter --"

# Use a temporary cache directory so the walkthrough never pollutes .shatter/
# in the repo (reserved for running shatter on shatter itself).
export SHATTER_CACHE_DIR
SHATTER_CACHE_DIR="$(mktemp -d "${TMPDIR:-/tmp}/shatter-demo-cache.XXXXXX")"

# Error tracking: collect failures for a summary at the end.
ERROR_LOG="$(mktemp "${TMPDIR:-/tmp}/shatter-walkthrough-errors.XXXXXX")"
STEP_ERRORS=0

cleanup() { rm -rf "$SHATTER_CACHE_DIR" "$ERROR_LOG"; }
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

usage() {
    cat <<EOF
${BOLD}Shatter Walkthrough${RESET} — exercise shatter's pipeline against example code

${BOLD}USAGE${RESET}
    ./demo/walkthrough.sh [OPTIONS]

${BOLD}OPTIONS${RESET}
    --interactive   Pause after each step (press Enter to continue)
    --auto          (no-op, auto is the default)
    --delay N       Seconds between steps in auto mode (default: 2)
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

    if [[ "$DRY_RUN" == true ]]; then
        echo "${DIM}  (dry-run: skipped)${RESET}"
    else
        # Capture combined output (stdout+stderr) while still displaying it.
        # We scan the captured output afterward for error indicators.
        local output_tmp
        output_tmp="$(mktemp)"
        if "$@" </dev/null > >(tee -a "$output_tmp") 2> >(tee -a "$output_tmp" >&2); then
            true
        else
            local rc=$?
            echo ""
            echo "${RED}  Command exited with status ${rc}${RESET}"
            echo "  Step ${CURRENT_STEP}: exit code ${rc}" >> "$ERROR_LOG"
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

EXAMPLES=(
    "examples/standalone/ts/01-arithmetic.ts:classifyNumber"
    "examples/standalone/ts/02-strings.ts:classifyString"
    "examples/standalone/ts/03-objects.ts:categorizeUser"
    "examples/standalone/ts/04-errors.ts:safeDivide"
)

GO_EXAMPLES=(
    "examples/standalone/go/01-arithmetic.go:ClassifyNumber"
    "examples/standalone/go/02-strings.go:ClassifyString"
    "examples/standalone/go/04-errors.go:SafeDivide"
)

RUST_EXAMPLES=(
    "examples/standalone/rust/01_arithmetic.rs:classify_number"
    "examples/standalone/rust/02_strings.rs:classify_string"
    "examples/standalone/rust/04_errors.rs:safe_divide"
)

TOTAL=43

# ─── Walkthrough ──────────────────────────────────────────────────────

echo ""
echo "${BOLD}${GREEN}Shatter Walkthrough${RESET}"
echo "${DIM}Exercising shatter's pipeline against ${#EXAMPLES[@]} TS + ${#GO_EXAMPLES[@]} Go + ${#RUST_EXAMPLES[@]} Rust example functions${RESET}"
if [[ "$DRY_RUN" == true ]]; then
    echo "${YELLOW}(dry-run mode: commands will not be executed)${RESET}"
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
    $SHATTER explore "${GO_EXAMPLES[@]}"

# Rust frontend is a separate binary — ensure it's up to date.
cargo build --manifest-path shatter-rust/Cargo.toml --quiet

# Stage 9: Analyze Rust functions
step 9 $TOTAL "Analyze Rust Functions" \
    "Discover parameters, types, and branch conditions in Rust code" \
    $SHATTER explore --analyze-only "${RUST_EXAMPLES[@]}"

# Stage 10: Explore Rust functions
step 10 $TOTAL "Explore Rust Functions" \
    "Concolic execution on Rust: generate inputs to cover all branches" \
    $SHATTER explore "${RUST_EXAMPLES[@]}"

# Stage 11: Scan Rust examples (project with deps)
step 11 $TOTAL "Scan Rust Examples" \
    "Scan Rust example directory in dependency order" \
    $SHATTER scan examples/rust/src

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
    "Show per-function timing data with --perf" \
    $SHATTER explore --perf "${EXAMPLES[@]}"

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
    bash -c "$SHATTER explore --spec-json 'demo/fixtures/arithmetic-v1.ts:classifyNumber' > /tmp/shatter-spec-old.json 2>/dev/null && $SHATTER explore --spec-json 'demo/fixtures/arithmetic-v2.ts:classifyNumber' > /tmp/shatter-spec-new.json 2>/dev/null && { $SHATTER spec-diff /tmp/shatter-spec-old.json /tmp/shatter-spec-new.json; true; }"

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
step 35 $TOTAL "Concolic Exploration (Z3)" \
    "Use the Z3-backed concolic explorer to solve branch constraints" \
    $SHATTER explore --concolic "${EXAMPLES[0]}"

# Stage 36: Concolic exploration of string functions (Z3 string ops)
step 36 $TOTAL "Concolic String Exploration (Z3)" \
    "Use the Z3-backed concolic explorer on string-method functions (startsWith, includes)" \
    $SHATTER explore --concolic "examples/standalone/ts/02-strings.ts:classifyString"

# Stage 37: Custom build-frontend help
step 37 $TOTAL "Custom Build Frontend" \
    "Show the build-frontend subcommand for compiling native generators into a custom frontend binary" \
    $SHATTER build-frontend --help

# Stage 38: Spec output to file (--output)
step 38 $TOTAL "Spec Output to File" \
    "Write a spec bundle to a JSON file with --output (includes fingerprints)" \
    $SHATTER explore --output /tmp/shatter-spec.json "${EXAMPLES[0]}"

# Stage 39: Incremental re-run (skips fresh functions)
step 39 $TOTAL "Incremental Re-run" \
    "Re-run with --output against existing spec — unchanged functions are skipped" \
    $SHATTER explore --output /tmp/shatter-spec.json "${EXAMPLES[0]}"

# Stage 40: Dry-run mode
step 40 $TOTAL "Dry-Run Mode" \
    "Use --dry-run to preview which functions would be re-explored without actually exploring" \
    $SHATTER explore --output /tmp/shatter-spec.json --dry-run "${EXAMPLES[0]}"

# Stage 41: Clean re-exploration
step 41 $TOTAL "Clean Re-exploration" \
    "Use --clean to force full re-exploration, ignoring the existing spec" \
    $SHATTER explore --output /tmp/shatter-spec.json --clean "${EXAMPLES[0]}"

# Stage 42: Stale command
# The spec from step 38 only explored classifyNumber. The file also exports
# compareMagnitudes, so `stale` correctly reports it as stale. Exit code 1
# means "some functions are stale or removed" — this is informational, not a failure.
step 42 $TOTAL "Stale Check" \
    "Check staleness relative to spec from step 38 (exit 1 = stale found, expected here)" \
    bash -c "$SHATTER stale 'examples/standalone/ts/01-arithmetic.ts' /tmp/shatter-spec.json; echo '(exit code 1 is expected: compareMagnitudes was not in the spec from step 38)'"

# Stage 43: Revalidate command
# Re-execute cached behaviors to check for drift/regressions. Uses cache
# populated by earlier explore steps. Exit code 0 = no regressions found.
step 43 $TOTAL "Revalidate" \
    "Revalidate cached behaviors for the arithmetic example" \
    $SHATTER revalidate 'examples/standalone/ts/01-arithmetic.ts'

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
