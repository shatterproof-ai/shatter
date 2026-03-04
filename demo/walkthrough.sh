#!/usr/bin/env bash
set -euo pipefail

# Shatter Walkthrough / Demo
# Exercises shatter's full pipeline against example code, showing output at each stage.
#
# Usage:
#   ./demo/walkthrough.sh              # Interactive: pauses after each step
#   ./demo/walkthrough.sh --auto       # Auto-advance: runs all steps continuously
#   ./demo/walkthrough.sh --auto --delay 3  # Auto with N-second delay between steps
#   ./demo/walkthrough.sh --dry-run    # Print commands without executing them

MODE="interactive"
DELAY=2
DRY_RUN=false
SHATTER="cargo run --quiet --bin shatter --"

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
    --auto          Run all steps without pausing
    --delay N       Seconds between steps in auto mode (default: 2)
    --dry-run       Print commands without executing them
    --help, -h      Show this help

${BOLD}MODES${RESET}
    Interactive (default)   Pauses after each step, press Enter to continue
    Auto                    Runs continuously with optional delay
    Dry-run                 Shows what would run, useful before core is built
EOF
    exit 0
}

# Parse args
while [[ $# -gt 0 ]]; do
    case "$1" in
        --auto)    MODE="auto"; shift ;;
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
        if "$@"; then
            true
        else
            local rc=$?
            echo ""
            echo "${RED}  Command exited with status ${rc}${RESET}"
        fi
    fi
    echo ""
}

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
    banner "$num" "$total" "$title" "$desc"
    run_cmd "$@"
    pause
}

# ─── Example targets ──────────────────────────────────────────────────

EXAMPLES=(
    "examples/typescript/src/01-arithmetic.ts:classifyNumber"
    "examples/typescript/src/02-strings.ts:classifyString"
    "examples/typescript/src/03-objects.ts:categorizeUser"
    "examples/typescript/src/04-errors.ts:safeDivide"
)

GO_EXAMPLES=(
    "examples/go/01-arithmetic.go:ClassifyNumber"
    "examples/go/02-strings.go:ClassifyString"
    "examples/go/03-errors.go:SafeDivide"
)

TOTAL=33

# ─── Walkthrough ──────────────────────────────────────────────────────

echo ""
echo "${BOLD}${GREEN}Shatter Walkthrough${RESET}"
echo "${DIM}Exercising shatter's pipeline against ${#EXAMPLES[@]} TS + ${#GO_EXAMPLES[@]} Go example functions${RESET}"
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

# Stage 5: Scan (dependency-ordered exploration)
step 5 $TOTAL "Scan in Dependency Order" \
    "Test functions leaf-first, using behavior maps as mocks for callers" \
    $SHATTER scan examples/typescript/src

# Stage 6: Cache behavior maps
step 6 $TOTAL "Explore with Disk Cache" \
    "Persist behavior maps to disk for reuse across runs" \
    $SHATTER explore --cache-dir /tmp/shatter-demo-cache "${EXAMPLES[@]}"

# Stage 7: Analyze Go functions
step 7 $TOTAL "Analyze Go Functions" \
    "Discover parameters, types, and branch conditions in Go code" \
    $SHATTER explore --analyze-only "${GO_EXAMPLES[@]}"

# Stage 8: Explore Go functions
step 8 $TOTAL "Explore Go Functions" \
    "Concolic execution on Go: generate inputs to cover all branches" \
    $SHATTER explore "${GO_EXAMPLES[@]}"

# Stage 9: Export tests
step 9 $TOTAL "Export Generated Tests" \
    "Generate Jest test files from explored behavior maps" \
    $SHATTER export-tests --framework jest --module-path "./src/01-arithmetic" \
    "examples/typescript/src/01-arithmetic.ts:classifyNumber"

# Stage 10: Run (full pipeline, analyze only)
step 10 $TOTAL "Run: Analyze Only" \
    "Discover, analyze, and report on all files in the examples directory" \
    $SHATTER run --analyze-only examples/typescript/src

# Stage 11: Run (full pipeline with exploration)
step 11 $TOTAL "Run: Full Pipeline" \
    "Discover, analyze, explore, and generate a full report" \
    $SHATTER run --max-iterations 10 --timeout 60 examples/typescript/src

# Stage 12: Log level verbosity (debug)
step 12 $TOTAL "Verbose Output with Debug Log Level" \
    "Show detailed progress output using --log-level debug" \
    $SHATTER explore --log-level debug "${EXAMPLES[0]}"

# Stage 13: Request timeout
step 13 $TOTAL "Request Timeout" \
    "Set a per-request timeout to bound frontend communication" \
    $SHATTER explore --request-timeout 15 "${EXAMPLES[@]}"

# Stage 14: User-provided inputs via config
step 14 $TOTAL "User-Provided Inputs via Config" \
    "Load candidate inputs from a .shatter config directory" \
    $SHATTER explore --config examples/typescript/.shatter/config.yaml \
    "${EXAMPLES[0]}"

# Stage 15: Performance stats
step 15 $TOTAL "Performance Stats" \
    "Show per-function timing data with --perf" \
    $SHATTER explore --perf "${EXAMPLES[@]}"

# Stage 16: Parallel scan with worker pool
step 16 $TOTAL "Parallel Scan" \
    "Scan with multiple worker processes for faster exploration" \
    $SHATTER scan --parallelism 2 --timeout-per-fn 30 examples/typescript/src

# Stage 17: Execution timeout
step 17 $TOTAL "Execution Timeout" \
    "Configure per-execution timeout passed to frontends" \
    $SHATTER explore --exec-timeout 5 --build-timeout 20 "${EXAMPLES[0]}"

# Stage 18: Go execution timeout
step 18 $TOTAL "Go Execution Timeout" \
    "Configurable timeouts also apply to the Go frontend" \
    $SHATTER explore --exec-timeout 8 "${GO_EXAMPLES[0]}"

# Stage 19: Scan with total timeout
step 19 $TOTAL "Scan Total Timeout" \
    "Bound overall scan wall-clock time with --timeout-total" \
    $SHATTER scan --timeout-total 120 --timeout-per-fn 30 examples/typescript/src

# Stage 20: Memory limit
step 20 $TOTAL "Memory Limit" \
    "Cap frontend memory usage (sets --max-old-space-size for TS, GOMEMLIMIT for Go)" \
    $SHATTER explore --memory-limit 512 "${EXAMPLES[0]}"

# Stage 21: Behavioral specification (markdown)
step 21 $TOTAL "Behavioral Specification (Markdown)" \
    "Generate a behavioral spec with equivalence classes, pre/postconditions" \
    $SHATTER explore --spec "${EXAMPLES[0]}"

# Stage 22: Behavioral specification (JSON)
step 22 $TOTAL "Behavioral Specification (JSON)" \
    "Machine-readable JSON spec for tooling integration" \
    $SHATTER explore --spec-json "${EXAMPLES[0]}"

# Stage 23: Spec diff
# Generate two spec JSON files and diff them. We use the same function twice
# (identical specs) so the diff shows "No changes detected" — a real diff
# would compare specs from different code versions.
step 23 $TOTAL "Specification Diff" \
    "Compare two spec JSON files to detect behavioral regressions" \
    bash -c "$SHATTER explore --spec-json '${EXAMPLES[0]}' > /tmp/shatter-spec-old.json 2>/dev/null && cp /tmp/shatter-spec-old.json /tmp/shatter-spec-new.json && $SHATTER spec-diff /tmp/shatter-spec-old.json /tmp/shatter-spec-new.json"

# Stage 24: Explore without boundary values
step 24 $TOTAL "Explore Without Boundary Values" \
    "Disable built-in boundary value seeding with --no-boundary-values" \
    $SHATTER explore --no-boundary-values "${EXAMPLES[0]}"

# Stage 25: Emit tests from scan
step 25 $TOTAL "Emit Tests from Scan" \
    "Generate Jest test files from behavior maps discovered during scan" \
    $SHATTER scan --emit-tests jest --output /tmp/shatter-demo-tests \
    examples/typescript/src

# Stage 26: Markdown scan report
step 26 $TOTAL "Markdown Scan Report" \
    "Generate a human-readable markdown report alongside JSON" \
    $SHATTER scan --format=markdown examples/typescript/src

# Stage 27: Scan dry-run
step 27 $TOTAL "Scan Dry Run" \
    "Preview which files would be scanned without executing" \
    $SHATTER scan --dry-run --language typescript examples/typescript/src

# Stage 28: Invariant detection
step 28 $TOTAL "Invariant Detection" \
    "Detect Daikon-style invariants over explored executions" \
    $SHATTER explore --invariants "${EXAMPLES[0]}"

# Stage 29: Setup + generators via config
step 29 $TOTAL "Setup + Generators via Config" \
    "Explore with setup/teardown lifecycle and custom type generators from .shatter/config.yaml" \
    $SHATTER explore --config examples/typescript/.shatter/config.yaml \
    "examples/typescript/src/03-objects.ts:categorizeUser"

# Stage 30: Setup + generators with debug logging
step 30 $TOTAL "Setup + Generators (Debug)" \
    "Show setup/teardown and generator lifecycle with --log-level debug" \
    $SHATTER explore --config examples/typescript/.shatter/config.yaml \
    --log-level debug "examples/typescript/src/03-objects.ts:categorizeUser"

# Stage 31: File-level explore (all exported functions)
step 31 $TOTAL "File-Level Explore" \
    "Explore all exported functions in a file by passing just the file path" \
    $SHATTER explore examples/typescript/src/01-arithmetic.ts

# Stage 32: Concolic exploration (Z3-backed)
step 32 $TOTAL "Concolic Exploration (Z3)" \
    "Use the Z3-backed concolic explorer to solve branch constraints" \
    $SHATTER explore --concolic "${EXAMPLES[0]}"

# Stage 33: Custom build-frontend help
step 33 $TOTAL "Custom Build Frontend" \
    "Show the build-frontend subcommand for compiling native generators into a custom frontend binary" \
    $SHATTER build-frontend --help

echo "${BOLD}${GREEN}Walkthrough complete.${RESET}"
