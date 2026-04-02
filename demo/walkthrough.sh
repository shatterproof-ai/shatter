#!/usr/bin/env bash
set -euo pipefail

# Shatter Walkthrough — Compact Demo
# Reads step definitions from demo/walkthrough.yaml (single source of truth).
# For the exhaustive coverage run, see demo/gauntlet.sh.
#
# Usage:
#   ./demo/walkthrough.sh              # Auto-advance: runs all steps continuously
#   ./demo/walkthrough.sh --interactive # Pauses after each step
#   ./demo/walkthrough.sh --delay 3    # Auto with N-second delay between steps
#   ./demo/walkthrough.sh --dry-run    # Print commands without executing them

MODE="auto"
DELAY=2
DRY_RUN=false
STEP_TIMEOUT=120

# Temp dirs — isolated from repo state and concurrent cargo commands
export SHATTER_CACHE_DIR SHATTER_SEEDS_DIR RUST_BACKTRACE CARGO_NET_OFFLINE CARGO_TARGET_DIR
SHATTER_CACHE_DIR="$(mktemp -d "${TMPDIR:-/tmp}/shatter-walkthrough-cache.XXXXXX")"
SHATTER_SEEDS_DIR="${SHATTER_CACHE_DIR}/seeds"
RUST_BACKTRACE="${RUST_BACKTRACE:-1}"
CARGO_NET_OFFLINE="${CARGO_NET_OFFLINE:-true}"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$(mktemp -d "${TMPDIR:-/tmp}/shatter-walkthrough-cargo.XXXXXX")}"

EXAMPLES_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/shatter-walkthrough-examples.XXXXXX")"
EXAMPLES_REPO_URL="${SHATTER_EXAMPLES_REPO:-https://github.com/shatterproof-ai/examples.git}"
EXAMPLES_REPO_REF="${SHATTER_EXAMPLES_REF:-}"

ERROR_LOG="$(mktemp "${TMPDIR:-/tmp}/shatter-walkthrough-errors.XXXXXX")"
STEP_ERRORS=0

cleanup() { rm -rf "$SHATTER_CACHE_DIR" "$ERROR_LOG" "$CARGO_TARGET_DIR" "$EXAMPLES_ROOT" || true; }
trap cleanup EXIT

if command -v gcc &>/dev/null; then
    export BINDGEN_EXTRA_CLANG_ARGS="${BINDGEN_EXTRA_CLANG_ARGS:-} -I$(gcc -print-file-name=include)"
fi

# Color support
if [[ -t 1 ]]; then
    BOLD=$'\033[1m' DIM=$'\033[2m' GREEN=$'\033[32m' CYAN=$'\033[36m'
    YELLOW=$'\033[33m' RED=$'\033[31m' RESET=$'\033[0m'
    SHATTER_COLOR="always"
else
    BOLD="" DIM="" GREEN="" CYAN="" YELLOW="" RED="" RESET=""
    SHATTER_COLOR="never"
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Resolve examples submodule ref
if [[ -z "$EXAMPLES_REPO_REF" ]]; then
    EXAMPLES_REPO_REF="$(git -C "$REPO_ROOT" ls-tree HEAD examples 2>/dev/null | awk '$1 == "160000" { print $3; exit }')"
fi

example_path() {
    local path="$1"
    if [[ "$path" == examples/* ]]; then
        printf '%s\n' "$EXAMPLES_ROOT/${path#examples/}"
    else
        printf '%s\n' "$path"
    fi
}

usage() {
    cat <<EOF
${BOLD}Shatter Walkthrough${RESET} — compact demo of the Shatter pipeline

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

while [[ $# -gt 0 ]]; do
    case "$1" in
        --auto)        MODE="auto"; shift ;;
        --interactive) MODE="interactive"; shift ;;
        --delay)       DELAY="$2"; shift 2 ;;
        --step-timeout) STEP_TIMEOUT="$2"; shift 2 ;;
        --dry-run)     DRY_RUN=true; shift ;;
        --help|-h)     usage ;;
        *)             echo "${RED}Unknown option: $1${RESET}"; exit 1 ;;
    esac
done

# Check dependencies
python3 -c "import yaml" 2>/dev/null || { echo "${RED}PyYAML required: pip install pyyaml${RESET}"; exit 1; }

# Resolve shatter binary
if [[ -n "${SHATTER_BIN:-}" ]]; then
    SHATTER="$SHATTER_BIN"
elif [[ -x "$REPO_ROOT/target/debug/shatter" ]]; then
    SHATTER="$REPO_ROOT/target/debug/shatter"
else
    echo "${RED}shatter binary not found.${RESET}"
    echo "Build first with: cargo build --bin shatter"
    echo "Or set SHATTER_BIN=/path/to/shatter"
    exit 1
fi

# Clone examples
echo "${YELLOW}Cloning clean examples checkout...${RESET}"
if ! git clone --quiet "$EXAMPLES_REPO_URL" "$EXAMPLES_ROOT"; then
    echo "${RED}failed to clone examples from ${EXAMPLES_REPO_URL}${RESET}"; exit 1
fi
if [[ -n "$EXAMPLES_REPO_REF" ]]; then
    if ! git -C "$EXAMPLES_ROOT" checkout --quiet "$EXAMPLES_REPO_REF"; then
        echo "${RED}failed to checkout examples revision ${EXAMPLES_REPO_REF}${RESET}"; exit 1
    fi
fi

# ─── Step execution helpers ──────────────────────────────────────────

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
            grep -iE "$error_pattern" "$output_tmp" | sed 's/^/    /' >> "$ERROR_LOG"
            STEP_ERRORS=$((STEP_ERRORS + 1))
        fi
        rm -f "$output_tmp"
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

CURRENT_STEP=0

# ─── Parse manifest and run steps ────────────────────────────────────

MANIFEST="$SCRIPT_DIR/walkthrough.yaml"
SAMPLE_MANIFEST="$REPO_ROOT/benchmarks/sample-manifest.json"

# Python helper: parse the YAML manifest, resolve target references,
# and output tab-delimited step records for the bash loop.
read_manifest() {
    python3 - "$MANIFEST" "$SAMPLE_MANIFEST" "$EXAMPLES_ROOT" <<'PY'
import json
import sys
import yaml

manifest_path, sample_path, examples_root = sys.argv[1], sys.argv[2], sys.argv[3]

with open(manifest_path, "r") as fh:
    manifest = yaml.safe_load(fh)

with open(sample_path, "r") as fh:
    samples = json.load(fh)

def resolve_sample_key(dotted_key):
    value = samples
    for part in dotted_key.split("."):
        value = value[part]
    return value

def remap_path(path):
    """Remap examples/... paths to the cloned checkout."""
    if path.startswith("examples/"):
        return examples_root + "/" + path[len("examples/"):]
    return path

def remap_target(target):
    """Remap the file portion of file:function targets."""
    if ":" in target:
        file_part, func_part = target.rsplit(":", 1)
        return remap_path(file_part) + ":" + func_part
    return remap_path(target)

total = len(manifest["steps"])

for i, step in enumerate(manifest["steps"], 1):
    title = step["title"]
    desc = step["description"]
    command = step["command"]

    # Resolve targets
    targets = []
    if "targets" in step:
        targets = [remap_target(t) for t in resolve_sample_key(step["targets"])]
    elif "targets_first" in step:
        group = resolve_sample_key(step["targets_first"])
        targets = [remap_target(group[0])]
    elif "targets_literal" in step:
        targets = [remap_target(t) for t in step["targets_literal"]]

    # Resolve additional args
    args = [remap_path(a) for a in step.get("args", [])]

    # Output: num \t total \t title \t desc \t command+args+targets (space-separated)
    full_cmd = command
    if args:
        full_cmd += " " + " ".join(args)
    if targets:
        full_cmd += " " + " ".join(targets)

    # Use null byte as record separator to handle spaces in paths
    print(f"{i}\t{total}\t{title}\t{desc}\t{full_cmd}", flush=True)
PY
}

echo ""
echo "${BOLD}${GREEN}Shatter Walkthrough${RESET}"
echo "${DIM}Compact demo of the Shatter pipeline${RESET}"
if [[ "$DRY_RUN" == true ]]; then
    echo "${YELLOW}(dry-run mode: commands will not be executed)${RESET}"
fi

while IFS=$'\t' read -r num total title desc full_cmd; do
    CURRENT_STEP="$num"
    banner "$num" "$total" "$title" "$desc"
    # shellcheck disable=SC2086
    run_cmd $SHATTER $full_cmd
    pause
done < <(read_manifest)

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
    echo "${BOLD}${GREEN}Walkthrough complete. All steps passed.${RESET}"
fi
