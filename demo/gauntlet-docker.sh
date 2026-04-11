#!/usr/bin/env bash
set -euo pipefail

# Shatter Docker Gauntlet
# Runs the gauntlet inside the distributable Docker image, validating the
# actual artifact users receive.
#
# Usage:
#   ./demo/gauntlet-docker.sh                  # Build image and run all steps
#   ./demo/gauntlet-docker.sh --image shatter  # Use a pre-built image
#   ./demo/gauntlet-docker.sh --interactive     # Pause after each step
#   ./demo/gauntlet-docker.sh --delay 3        # Auto with N-second delay
#   ./demo/gauntlet-docker.sh --dry-run        # Print commands without executing

MODE="auto"
DELAY=2
DRY_RUN=false
IMAGE=""
IMAGE_DEFAULT="shatter-gauntlet"
EXAMPLES_ROOT=""

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
${BOLD}Shatter Docker Gauntlet${RESET} — broad CLI coverage run inside the Docker image

${BOLD}USAGE${RESET}
    ./demo/gauntlet-docker.sh [OPTIONS]

${BOLD}OPTIONS${RESET}
    --image NAME    Use a pre-built Docker image (skip build)
    --interactive   Pause after each step (press Enter to continue)
    --auto          (no-op, auto is the default)
    --delay N       Seconds between steps in auto mode (default: 2)
    --dry-run       Print commands without executing them
    --help, -h      Show this help

${BOLD}MODES${RESET}
    Auto (default)          Runs continuously with optional delay
    Interactive             Pauses after each step, press Enter to continue
    Dry-run                 Shows what would run

${BOLD}NOTES${RESET}
    Some gauntlet steps are skipped in container mode (e.g. cargo build,
    custom build-frontend). The container image includes pre-built binaries.
EOF
    exit 0
}

# Parse args
while [[ $# -gt 0 ]]; do
    case "$1" in
        --auto)        MODE="auto"; shift ;;
        --interactive) MODE="interactive"; shift ;;
        --delay)       DELAY="$2"; shift 2 ;;
        --dry-run)     DRY_RUN=true; shift ;;
        --image)       IMAGE="$2"; shift 2 ;;
        --help|-h)     usage ;;
        *)             echo "${RED}Unknown option: $1${RESET}"; echo "Run with --help for usage."; exit 1 ;;
    esac
done

# ─── Docker availability ─────────────────────────────────────────────
if ! command -v docker &>/dev/null; then
    echo "${RED}Error: docker is not installed or not in PATH.${RESET}"
    echo "Install Docker and try again: https://docs.docker.com/get-docker/"
    exit 1
fi

# ─── Resolve repo root ───────────────────────────────────────────────
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

# ─── Build or reuse image ────────────────────────────────────────────
if [[ -n "$IMAGE" ]]; then
    echo "${DIM}Using pre-built image: ${IMAGE}${RESET}"
else
    IMAGE="$IMAGE_DEFAULT"
    echo "${BOLD}Building Docker image '${IMAGE}'...${RESET}"
    if [[ "$DRY_RUN" == true ]]; then
        echo "${DIM}  (dry-run: skipped)${RESET}"
    else
        docker build -t "$IMAGE" "$REPO_ROOT"
    fi
fi

# ─── Error tracking ──────────────────────────────────────────────────
ERROR_LOG="$(mktemp "${TMPDIR:-/tmp}/shatter-docker-gauntlet-errors.XXXXXX")"
STEP_ERRORS=0
CACHE_VOL="shatter-demo-cache-$$"

cleanup() {
    rm -f "$ERROR_LOG"
    docker volume rm "$CACHE_VOL" &>/dev/null || true
    rm -rf "$EXAMPLES_ROOT" || true
}
trap cleanup EXIT

# Create a named volume for cache persistence across steps
if [[ "$DRY_RUN" != true ]]; then
    docker volume create "$CACHE_VOL" >/dev/null
fi

echo "${YELLOW}Cloning clean examples checkout...${RESET}"
if [[ "$DRY_RUN" != true ]]; then
    if ! EXAMPLES_ROOT="$(python3 "$REPO_ROOT/scripts/examples_checkout.py" --fresh)"; then
        echo "${RED}failed to prepare examples checkout${RESET}"
        exit 1
    fi
else
    EXAMPLES_ROOT="/tmp/shatter-examples.dry-run"
fi

# ─── Helpers ──────────────────────────────────────────────────────────
banner() {
    local num="$1" total="$2" title="$3" desc="$4"
    echo ""
    echo "${BOLD}${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
    echo "${BOLD}  Step ${num}/${total} · ${title}${RESET}"
    echo "${DIM}  ${desc}${RESET}"
    echo "${BOLD}${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
    echo ""
}

# Run a shatter command inside the Docker container.
# Usage: docker_run [shatter args...]
docker_run() {
    docker run --rm \
        -v "${EXAMPLES_ROOT}:/repo/examples:ro" \
        -v "${CACHE_VOL}:/cache" \
        -e "SHATTER_CACHE_DIR=/cache" \
        "$IMAGE" \
        "$@"
}

# Run a command, capture output, scan for errors — mirrors gauntlet.sh run_cmd().
run_cmd() {
    echo "${DIM}\$${RESET} ${YELLOW}shatter $*${RESET}"
    echo ""

    if [[ "$DRY_RUN" == true ]]; then
        echo "${DIM}  (dry-run: skipped)${RESET}"
    else
        local output_tmp
        output_tmp="$(mktemp)"
        if docker_run "$@" </dev/null > >(tee -a "$output_tmp") 2> >(tee -a "$output_tmp" >&2); then
            true
        else
            local rc=$?
            echo ""
            echo "${RED}  Command exited with status ${rc}${RESET}"
            echo "  Step ${CURRENT_STEP}: exit code ${rc}" >> "$ERROR_LOG"
            STEP_ERRORS=$((STEP_ERRORS + 1))
        fi
        wait 2>/dev/null || true
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

skip() {
    local num="$1" total="$2" title="$3" reason="$4"
    echo ""
    echo "${DIM}  Step ${num}/${total} · ${title} — SKIPPED (${reason})${RESET}"
    echo ""
}

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

# ─── Example targets ─────────────────────────────────────────────────
# Include one branch-dense "advanced" example in the guided demo. The other
# new mirrored examples stay in scan coverage to keep the gauntlet readable.
mapfile -t EXAMPLES < <(load_sample_group "walkthrough.typescript")
mapfile -t GO_EXAMPLES < <(load_sample_group "walkthrough.go")
mapfile -t RUST_EXAMPLES < <(load_sample_group "walkthrough.rust")

TOTAL=42

# ─── Gauntlet ────────────────────────────────────────────────────────

echo ""
echo "${BOLD}${GREEN}Shatter Docker Gauntlet${RESET}"
echo "${DIM}Running inside Docker image '${IMAGE}'${RESET}"
echo "${DIM}Exercising shatter's pipeline against ${#EXAMPLES[@]} TS + ${#GO_EXAMPLES[@]} Go + ${#RUST_EXAMPLES[@]} Rust example functions${RESET}"
if [[ "$DRY_RUN" == true ]]; then
    echo "${YELLOW}(dry-run mode: commands will not be executed)${RESET}"
fi

# Stage 1: Analyze
step 1 $TOTAL "Analyze Target Functions" \
    "Discover parameters, types, and branch conditions" \
    explore --analyze-only "${EXAMPLES[@]}"

# Stage 2: Analyze with scope config
skip 2 $TOTAL "Analyze with Scope Config" "scope config not mounted"

# Stage 3: Explore
step 3 $TOTAL "Generate & Execute Inputs" \
    "Concolic execution: generate inputs to cover all branches" \
    explore "${EXAMPLES[@]}"

# Stage 4: Clusters
step 4 $TOTAL "Show Behavior Clusters" \
    "Group executions by branch path into distinct behaviors" \
    explore --show-clusters "${EXAMPLES[@]}"

# Stage 5: Scan standalone TS files
step 5 $TOTAL "Scan Standalone TypeScript" \
    "Scan standalone TypeScript files (no project dependencies needed)" \
    scan examples/standalone/ts

# Stage 6: Cache behavior maps
step 6 $TOTAL "Explore with Disk Cache" \
    "Persist behavior maps to disk for reuse across runs (SHATTER_CACHE_DIR)" \
    explore "${EXAMPLES[@]}"

# Stage 7: Analyze Go functions
step 7 $TOTAL "Analyze Go Functions" \
    "Discover parameters, types, and branch conditions in Go code" \
    explore --analyze-only "${GO_EXAMPLES[@]}"

# Stage 8: Explore Go functions
step 8 $TOTAL "Explore Go Functions" \
    "Concolic execution on Go: generate inputs to cover all branches" \
    explore "${GO_EXAMPLES[@]}"

# (no cargo build needed — shatter-rust is pre-built in the image)

# Stage 9: Analyze Rust functions
step 9 $TOTAL "Analyze Rust Functions" \
    "Discover parameters, types, and branch conditions in Rust code" \
    explore --analyze-only "${RUST_EXAMPLES[@]}"

# Stage 10: Explore Rust functions
step 10 $TOTAL "Explore Rust Functions" \
    "Concolic execution on Rust: generate inputs to cover all branches" \
    explore "${RUST_EXAMPLES[@]}"

# Stage 11: Scan Rust examples
skip 11 $TOTAL "Scan Rust Examples" "Rust project deps not available in container"

# Stage 13: Run (analyze only)
step 13 $TOTAL "Run: Analyze Only" \
    "Discover, analyze, and report on all files in the standalone TS directory" \
    run --analyze-only examples/standalone/ts

# Stage 14: Run (full pipeline)
step 14 $TOTAL "Run: Full Pipeline" \
    "Discover, analyze, explore, and generate a full report" \
    run --max-iterations 10 --timeout 60 examples/standalone/ts

# Stage 15: Debug log level
step 15 $TOTAL "Verbose Output with Debug Log Level" \
    "Show detailed progress output using --log-level debug" \
    explore --log-level debug "${EXAMPLES[0]}"

# Stage 16: Request timeout
step 16 $TOTAL "Request Timeout" \
    "Set a per-request timeout to bound frontend communication" \
    explore --request-timeout 15 "${EXAMPLES[@]}"

# Stage 17: User-provided inputs via config
skip 17 $TOTAL "User-Provided Inputs via Config" "config dir not mounted"

# Stage 18: Performance stats
step 18 $TOTAL "Performance Stats" \
    "Show per-function timing data with --timing summary" \
    explore --timing summary "${EXAMPLES[@]}"

# Stage 19: Parallel scan
step 19 $TOTAL "Parallel Scan" \
    "Scan with multiple worker processes for faster exploration" \
    scan --parallelism 2 --timeout-per-fn 30 examples/standalone/ts

# Stage 20: Execution timeout
step 20 $TOTAL "Execution Timeout" \
    "Configure per-execution timeout passed to frontends" \
    explore --exec-timeout 5 --build-timeout 20 "${EXAMPLES[0]}"

# Stage 21: Go execution timeout
step 21 $TOTAL "Go Execution Timeout" \
    "Configurable timeouts also apply to the Go frontend" \
    explore --exec-timeout 8 "${GO_EXAMPLES[0]}"

# Stage 22: Scan with total timeout
step 22 $TOTAL "Scan Total Timeout" \
    "Bound overall scan wall-clock time with --timeout-total" \
    scan --timeout-total 120 --timeout-per-fn 30 examples/standalone/ts

# Stage 23: Memory limit
step 23 $TOTAL "Memory Limit" \
    "Cap frontend memory usage (sets --max-old-space-size for TS, GOMEMLIMIT for Go)" \
    explore --memory-limit 512 "${EXAMPLES[0]}"

# Stage 24: Behavioral specification (markdown)
step 24 $TOTAL "Behavioral Specification (Markdown)" \
    "Generate a behavioral spec with equivalence classes, pre/postconditions" \
    explore --spec "${EXAMPLES[0]}"

# Stage 25: Behavioral specification (JSON)
step 25 $TOTAL "Behavioral Specification (JSON)" \
    "Machine-readable JSON spec for tooling integration" \
    explore --spec-json "${EXAMPLES[0]}"

# Stage 26: Spec diff
# Fixtures are not mounted — skip
skip 26 $TOTAL "Specification Diff" "demo fixtures not mounted"

# Stage 27: Explore without boundary values
step 27 $TOTAL "Explore Without Boundary Values" \
    "Disable built-in boundary value seeding with --no-boundary-values" \
    explore --no-boundary-values "${EXAMPLES[0]}"

# Stage 29: Markdown scan report
step 29 $TOTAL "Markdown Scan Report" \
    "Generate a human-readable markdown report alongside JSON" \
    scan -o /tmp/shatter-scan-report.md examples/standalone/ts

# Stage 30: Scan dry-run
step 30 $TOTAL "Scan Dry Run" \
    "Preview which files would be scanned without executing" \
    scan --dry-run --language typescript examples/standalone/ts

# Stage 31: Invariant detection
step 31 $TOTAL "Invariant Detection" \
    "Detect Daikon-style invariants over explored executions" \
    explore --invariants "${EXAMPLES[0]}"

# Stage 32: Setup + generators via config
skip 32 $TOTAL "Setup + Generators via Config" "config dir not mounted"

# Stage 33: Setup + generators (debug)
skip 33 $TOTAL "Setup + Generators (Debug)" "config dir not mounted"

# Stage 34: File-level explore
step 34 $TOTAL "File-Level Explore" \
    "Explore all exported functions in a file by passing just the file path" \
    explore examples/standalone/ts/01-arithmetic.ts

# Stage 35: Concolic exploration (Z3)
step 35 $TOTAL "Concolic Exploration (Z3)" \
    "Use the Z3-backed concolic explorer to solve branch constraints" \
    explore --concolic "${EXAMPLES[0]}"

# Stage 36: Concolic string exploration (Z3)
step 36 $TOTAL "Concolic String Exploration (Z3)" \
    "Use the Z3-backed concolic explorer on string-method functions" \
    explore --concolic "examples/standalone/ts/02-strings.ts:classifyString"

# Stage 37: Custom build-frontend
skip 37 $TOTAL "Custom Build Frontend" "build-frontend requires cargo (not in runtime image)"

# Stage 38: Spec output to file
step 38 $TOTAL "Spec Output to File" \
    "Write a spec bundle to a JSON file with --output (includes fingerprints)" \
    explore --output /tmp/shatter-spec.json "${EXAMPLES[0]}"

# Stage 39: Incremental re-run
step 39 $TOTAL "Incremental Re-run" \
    "Re-run with --output against existing spec — unchanged functions are skipped" \
    explore --output /tmp/shatter-spec.json "${EXAMPLES[0]}"

# Stage 40: Dry-run mode
step 40 $TOTAL "Dry-Run Mode" \
    "Use --dry-run to preview which functions would be re-explored" \
    explore --output /tmp/shatter-spec.json --dry-run "${EXAMPLES[0]}"

# Stage 41: Clean re-exploration
step 41 $TOTAL "Clean Re-exploration" \
    "Use --clean to force full re-exploration, ignoring the existing spec" \
    explore --output /tmp/shatter-spec.json --clean "${EXAMPLES[0]}"

# Stage 42: Stale check
# The spec from step 38 is inside the container's /tmp — stale needs it there too.
# Since each docker run is a fresh container, the spec file from step 38 is gone.
skip 42 $TOTAL "Stale Check" "spec file from step 38 not persisted across containers"

# ─── Error Summary ───────────────────────────────────────────────────
if [[ -s "$ERROR_LOG" ]]; then
    echo ""
    echo "${BOLD}${RED}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
    echo "${BOLD}${RED}  ERROR SUMMARY (${STEP_ERRORS} issue(s))${RESET}"
    echo "${BOLD}${RED}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
    cat "$ERROR_LOG"
    echo ""
    echo "${BOLD}${GREEN}Docker gauntlet complete with errors.${RESET}"
    exit 1
else
    echo "${BOLD}${GREEN}Docker gauntlet complete. All steps passed.${RESET}"
fi
