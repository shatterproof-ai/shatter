#!/usr/bin/env bash
set -euo pipefail

# Shatter Walkthrough — Compact Demo (12 steps)
# Core product story: analyze → explore → scan → artifacts, with language parity
# across TypeScript, Go, and Rust.
# For broad CLI coverage (flag permutations, stress cases), see demo/gauntlet.sh.
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

# Color support (disabled if not a terminal)
if [[ -t 1 ]]; then
    BOLD=$'\033[1m'
    DIM=$'\033[2m'
    GREEN=$'\033[32m'
    CYAN=$'\033[36m'
    YELLOW=$'\033[33m'
    RED=$'\033[31m'
    RESET=$'\033[0m'
    SHATTER_COLOR="always"
else
    BOLD="" DIM="" GREEN="" CYAN="" YELLOW="" RED="" RESET=""
    SHATTER_COLOR="never"
fi

usage() {
    cat <<EOF
${BOLD}Shatter Walkthrough${RESET} — compact demo of the core product story

${BOLD}USAGE${RESET}
    ./demo/walkthrough.sh [OPTIONS]

${BOLD}OPTIONS${RESET}
    --interactive   Pause after each step (press Enter to continue)
    --auto          (no-op, auto is the default)
    --delay N       Seconds between steps in auto mode (default: 2)
    --step-timeout N  Per-step timeout in seconds (default: 120, 0 = no limit)
    --dry-run       Print commands without executing them
    --help, -h      Show this help
EOF
    exit 0
}

# Parse args early so --dry-run and --help work without prerequisites
while [[ $# -gt 0 ]]; do
    case "$1" in
        --auto)        MODE="auto"; shift ;;
        --interactive) MODE="interactive"; shift ;;
        --delay)       DELAY="$2"; shift 2 ;;
        --step-timeout) STEP_TIMEOUT="$2"; shift 2 ;;
        --dry-run)     DRY_RUN=true; shift ;;
        --help|-h)     usage ;;
        *)             echo "${RED}Unknown option: $1${RESET}"; echo "Run with --help for usage."; exit 1 ;;
    esac
done

# ─── Environment setup ───────────────────────────────────────────────
# Use temporary directories so the walkthrough never pollutes repo-local state
export SHATTER_CACHE_DIR SHATTER_SEEDS_DIR RUST_BACKTRACE XDG_CACHE_HOME GOCACHE CARGO_NET_OFFLINE CARGO_TARGET_DIR
SHATTER_CACHE_DIR="$(mktemp -d "${TMPDIR:-/tmp}/shatter-demo-cache.XXXXXX")"
SHATTER_SEEDS_DIR="${SHATTER_CACHE_DIR}/seeds"
RUST_BACKTRACE="${RUST_BACKTRACE:-1}"
XDG_CACHE_HOME="${XDG_CACHE_HOME:-$(mktemp -d "${TMPDIR:-/tmp}/shatter-demo-xdg.XXXXXX")}"
GOCACHE="${GOCACHE:-$(mktemp -d "${TMPDIR:-/tmp}/shatter-demo-gocache.XXXXXX")}"
CARGO_NET_OFFLINE="${CARGO_NET_OFFLINE:-true}"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$(mktemp -d "${TMPDIR:-/tmp}/shatter-demo-cargo-target.XXXXXX")}"

# Spec output directory for multi-step flows (steps 11-12)
SPEC_DIR="$(mktemp -d "${TMPDIR:-/tmp}/shatter-walkthrough-specs.XXXXXX")"

# Error tracking
ERROR_LOG="$(mktemp "${TMPDIR:-/tmp}/shatter-walkthrough-errors.XXXXXX")"
STEP_ERRORS=0

cleanup() { rm -rf "$SHATTER_CACHE_DIR" "$ERROR_LOG" "$XDG_CACHE_HOME" "$GOCACHE" "$CARGO_TARGET_DIR" "$SPEC_DIR" || true; }
trap cleanup EXIT

# Ensure bindgen can find stdbool.h via GCC's include path
if command -v gcc &>/dev/null; then
    export BINDGEN_EXTRA_CLANG_ARGS="${BINDGEN_EXTRA_CLANG_ARGS:-} -I$(gcc -print-file-name=include)"
fi

# ─── Examples checkout ────────────────────────────────────────────────
EXAMPLES_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/shatter-demo-examples.XXXXXX")"
EXAMPLES_REPO_URL="${SHATTER_EXAMPLES_REPO:-https://github.com/shatterproof-ai/examples.git}"
EXAMPLES_REPO_REF="${SHATTER_EXAMPLES_REF:-}"

# Append examples root to cleanup
cleanup() { rm -rf "$SHATTER_CACHE_DIR" "$ERROR_LOG" "$XDG_CACHE_HOME" "$GOCACHE" "$CARGO_TARGET_DIR" "$SPEC_DIR" "$EXAMPLES_ROOT" || true; }
trap cleanup EXIT

example_path() {
    local path="$1"
    if [[ "$path" == examples/* ]]; then
        printf '%s\n' "$EXAMPLES_ROOT/${path#examples/}"
    else
        printf '%s\n' "$path"
    fi
}

if [[ -z "$EXAMPLES_REPO_REF" ]]; then
    EXAMPLES_REPO_REF="$(git ls-tree HEAD examples 2>/dev/null | awk '$1 == "160000" { print $3; exit }')"
fi

if [[ "$DRY_RUN" != true ]]; then
    echo "${YELLOW}Cloning clean examples checkout...${RESET}"
    if ! git clone --quiet "$EXAMPLES_REPO_URL" "$EXAMPLES_ROOT"; then
        echo "${RED}failed to clone examples repository from ${EXAMPLES_REPO_URL}${RESET}"
        exit 1
    fi
    if [[ -n "$EXAMPLES_REPO_REF" ]]; then
        if ! git -C "$EXAMPLES_ROOT" checkout --quiet "$EXAMPLES_REPO_REF"; then
            echo "${RED}failed to checkout examples revision ${EXAMPLES_REPO_REF}${RESET}"
            exit 1
        fi
    fi
    if [[ ! -f "$(example_path "examples/standalone/ts/01-arithmetic.ts")" ]]; then
        echo "${RED}clean examples checkout is missing walkthrough fixtures${RESET}"
        exit 1
    fi
fi

# Pick one representative example per language for single-function steps
TS_EXAMPLE="$(example_path "examples/standalone/ts/01-arithmetic.ts:classifyNumber")"
GO_EXAMPLE="$(example_path "examples/standalone/go/01-arithmetic.go:ClassifyNumber")"
RUST_EXAMPLE="$(example_path "examples/standalone/rust/01_arithmetic.rs:classify_number")"
EXAMPLES_TS_DIR="$(example_path "examples/standalone/ts")"

# ─── Binary discovery ────────────────────────────────────────────────
if [[ "$DRY_RUN" != true ]]; then
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
else
    SHATTER="shatter"  # placeholder for dry-run display
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

run_cmd() {
    echo "${DIM}\$${RESET} ${YELLOW}$*${RESET}"
    echo ""

    local cmd=("$@")
    if [[ ${#cmd[@]} -ge 2 && "${cmd[0]}" == "$SHATTER" ]]; then
        cmd=("$SHATTER" --color "$SHATTER_COLOR" "${cmd[@]:1}")
    fi

    if [[ "$DRY_RUN" == true ]]; then
        echo "${DIM}  (dry-run: skipped)${RESET}"
    else
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

TOTAL=12

# ─── Walkthrough ─────────────────────────────────────────────────────

echo ""
echo "${BOLD}${GREEN}Shatter Walkthrough${RESET}"
echo "${DIM}Compact demo: analyze → explore → scan → artifacts${RESET}"
if [[ "$DRY_RUN" != true ]]; then
    echo "${DIM}Using clean examples checkout: ${EXAMPLES_ROOT}${RESET}"
fi
if [[ "$DRY_RUN" == true ]]; then
    echo "${YELLOW}(dry-run mode: commands will not be executed)${RESET}"
fi

# ─── Core Journey: Analyze (TS, Go, Rust) ────────────────────────────

step 1 $TOTAL "Analyze TypeScript" \
    "Discover parameters, types, and branch conditions in a TypeScript function" \
    $SHATTER explore --analyze-only "$TS_EXAMPLE"

step 2 $TOTAL "Analyze Go" \
    "Discover parameters, types, and branch conditions in a Go function" \
    $SHATTER explore --analyze-only "$GO_EXAMPLE"

step 3 $TOTAL "Analyze Rust" \
    "Discover parameters, types, and branch conditions in a Rust function" \
    $SHATTER explore --analyze-only "$RUST_EXAMPLE"

# ─── Core Journey: Explore (TS, Go, Rust) ────────────────────────────

step 4 $TOTAL "Explore TypeScript" \
    "Generate and execute inputs to cover all branches" \
    $SHATTER explore "$TS_EXAMPLE"

step 5 $TOTAL "Explore Go" \
    "Generate and execute inputs to cover all branches in Go" \
    $SHATTER explore --max-iterations 3 --timeout-explore 25 "$GO_EXAMPLE"

step 6 $TOTAL "Explore Rust" \
    "Generate and execute inputs to cover all branches in Rust" \
    $SHATTER explore --max-iterations 3 --timeout-explore 25 "$RUST_EXAMPLE"

# ─── Core Journey: Scan ──────────────────────────────────────────────
# Scan is currently supported for TypeScript standalone files.
# Go and Rust scan coverage lives in the gauntlet.

step 7 $TOTAL "Scan TypeScript Directory" \
    "Discover and explore all exported functions in a directory" \
    $SHATTER scan "$EXAMPLES_TS_DIR"

# ─── Feature Showcase ────────────────────────────────────────────────

step 8 $TOTAL "Concolic Exploration (Z3)" \
    "Use the Z3-backed concolic solver to find branch-covering inputs" \
    $SHATTER explore --concolic "$TS_EXAMPLE"

step 9 $TOTAL "Behavioral Specification" \
    "Generate a human-readable behavioral spec with equivalence classes" \
    $SHATTER explore --spec "$TS_EXAMPLE"

step 10 $TOTAL "Export Tests" \
    "Generate Jest test files from explored behavior maps" \
    $SHATTER export-tests --framework jest --module-path "./src/01-arithmetic" \
    "$TS_EXAMPLE"

# ─── Multi-Step: Incrementality ──────────────────────────────────────

step 11 $TOTAL "Spec to File" \
    "Write a spec bundle to a JSON file (includes fingerprints for incrementality)" \
    $SHATTER explore --output "$SPEC_DIR/spec.json" "$TS_EXAMPLE"

step 12 $TOTAL "Incremental Dry-Run" \
    "Preview which functions would be re-explored — unchanged ones are skipped" \
    $SHATTER explore --output "$SPEC_DIR/spec.json" --dry-run "$TS_EXAMPLE"

# ─── Error Summary ───────────────────────────────────────────────────
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
    echo ""
    echo "${BOLD}${GREEN}Walkthrough complete. All steps passed.${RESET}"
fi
