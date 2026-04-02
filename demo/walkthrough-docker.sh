#!/usr/bin/env bash
set -euo pipefail

# Shatter Docker Walkthrough — Compact Demo
# Mirrors the local walkthrough (demo/walkthrough.sh) inside the distributable
# Docker image, validating the actual artifact users receive.
#
# All 12 steps run — no skips. The compact walkthrough was designed to avoid
# Docker-incompatible features (scope configs, project deps, cargo builds).
# Multi-step flows use the persistent cache volume instead of /tmp.
#
# Usage:
#   ./demo/walkthrough-docker.sh                  # Build image and run all steps
#   ./demo/walkthrough-docker.sh --image shatter  # Use a pre-built image
#   ./demo/walkthrough-docker.sh --interactive     # Pause after each step
#   ./demo/walkthrough-docker.sh --delay 3        # Auto with N-second delay
#   ./demo/walkthrough-docker.sh --dry-run        # Print commands without executing

MODE="auto"
DELAY=2
DRY_RUN=false
IMAGE=""
IMAGE_DEFAULT="shatter-walkthrough"

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
${BOLD}Shatter Docker Walkthrough${RESET} — compact demo inside the Docker image

${BOLD}USAGE${RESET}
    ./demo/walkthrough-docker.sh [OPTIONS]

${BOLD}OPTIONS${RESET}
    --image NAME    Use a pre-built Docker image (skip build)
    --interactive   Pause after each step (press Enter to continue)
    --auto          (no-op, auto is the default)
    --delay N       Seconds between steps in auto mode (default: 2)
    --dry-run       Print commands without executing them
    --help, -h      Show this help

${BOLD}NOTES${RESET}
    All 12 walkthrough steps run in Docker — no skips. Multi-step flows
    use a persistent cache volume so artifacts survive across containers.
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
ERROR_LOG="$(mktemp "${TMPDIR:-/tmp}/shatter-docker-walkthrough-errors.XXXXXX")"
STEP_ERRORS=0
CACHE_VOL="shatter-walkthrough-cache-$$"

cleanup() {
    rm -f "$ERROR_LOG"
    docker volume rm "$CACHE_VOL" &>/dev/null || true
}
trap cleanup EXIT

# Create a named volume for persistence across steps.
# Used for SHATTER_CACHE_DIR and multi-step spec output.
if [[ "$DRY_RUN" != true ]]; then
    docker volume create "$CACHE_VOL" >/dev/null
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
# Mounts: examples (read-only), cache volume (read-write, persists across steps).
docker_run() {
    docker run --rm \
        -v "${REPO_ROOT}/examples:/repo/examples:ro" \
        -v "${CACHE_VOL}:/cache" \
        -e "SHATTER_CACHE_DIR=/cache" \
        "$IMAGE" \
        "$@"
}

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

# ─── Example targets ─────────────────────────────────────────────────
# Same examples as the local walkthrough. Paths are relative to /repo inside
# the container (examples are mounted at /repo/examples).
TS_EXAMPLE="examples/standalone/ts/01-arithmetic.ts:classifyNumber"
GO_EXAMPLE="examples/standalone/go/01-arithmetic.go:ClassifyNumber"
RUST_EXAMPLE="examples/standalone/rust/01_arithmetic.rs:classify_number"

TOTAL=12

# ─── Walkthrough ─────────────────────────────────────────────────────

echo ""
echo "${BOLD}${GREEN}Shatter Docker Walkthrough${RESET}"
echo "${DIM}Running inside Docker image '${IMAGE}'${RESET}"
echo "${DIM}Compact demo: analyze → explore → scan → artifacts${RESET}"
if [[ "$DRY_RUN" == true ]]; then
    echo "${YELLOW}(dry-run mode: commands will not be executed)${RESET}"
fi

# ─── Core Journey: Analyze (TS, Go, Rust) ────────────────────────────

step 1 $TOTAL "Analyze TypeScript" \
    "Discover parameters, types, and branch conditions in a TypeScript function" \
    explore --analyze-only "$TS_EXAMPLE"

step 2 $TOTAL "Analyze Go" \
    "Discover parameters, types, and branch conditions in a Go function" \
    explore --analyze-only "$GO_EXAMPLE"

step 3 $TOTAL "Analyze Rust" \
    "Discover parameters, types, and branch conditions in a Rust function" \
    explore --analyze-only "$RUST_EXAMPLE"

# ─── Core Journey: Explore (TS, Go, Rust) ────────────────────────────

step 4 $TOTAL "Explore TypeScript" \
    "Generate and execute inputs to cover all branches" \
    explore "$TS_EXAMPLE"

step 5 $TOTAL "Explore Go" \
    "Generate and execute inputs to cover all branches in Go" \
    explore --max-iterations 3 --timeout-explore 25 "$GO_EXAMPLE"

step 6 $TOTAL "Explore Rust" \
    "Generate and execute inputs to cover all branches in Rust" \
    explore --max-iterations 3 --timeout-explore 25 "$RUST_EXAMPLE"

# ─── Core Journey: Scan ──────────────────────────────────────────────
# Scan is currently supported for TypeScript standalone files.
# Go and Rust scan coverage lives in the gauntlet.

step 7 $TOTAL "Scan TypeScript Directory" \
    "Discover and explore all exported functions in a directory" \
    scan examples/standalone/ts

# ─── Feature Showcase ────────────────────────────────────────────────

step 8 $TOTAL "Concolic Exploration (Z3)" \
    "Use the Z3-backed concolic solver to find branch-covering inputs" \
    explore --concolic "$TS_EXAMPLE"

step 9 $TOTAL "Behavioral Specification" \
    "Generate a human-readable behavioral spec with equivalence classes" \
    explore --spec "$TS_EXAMPLE"

step 10 $TOTAL "Export Tests" \
    "Generate Jest test files from explored behavior maps" \
    export-tests --framework jest --module-path "./src/01-arithmetic" \
    "$TS_EXAMPLE"

# ─── Multi-Step: Incrementality ──────────────────────────────────────
# Uses /cache/ (the persistent named volume) so the spec file from step 11
# survives into step 12's fresh container.

step 11 $TOTAL "Spec to File" \
    "Write a spec bundle to a JSON file (includes fingerprints for incrementality)" \
    explore --output /cache/spec.json "$TS_EXAMPLE"

step 12 $TOTAL "Incremental Dry-Run" \
    "Preview which functions would be re-explored — unchanged ones are skipped" \
    explore --output /cache/spec.json --dry-run "$TS_EXAMPLE"

# ─── Error Summary ───────────────────────────────────────────────────
if [[ -s "$ERROR_LOG" ]]; then
    echo ""
    echo "${BOLD}${RED}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
    echo "${BOLD}${RED}  ERROR SUMMARY (${STEP_ERRORS} issue(s))${RESET}"
    echo "${BOLD}${RED}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
    cat "$ERROR_LOG"
    echo ""
    echo "${BOLD}${GREEN}Docker walkthrough complete with errors.${RESET}"
    exit 1
else
    echo ""
    echo "${BOLD}${GREEN}Docker walkthrough complete. All steps passed.${RESET}"
fi
