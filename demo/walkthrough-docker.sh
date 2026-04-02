#!/usr/bin/env bash
set -euo pipefail

# Shatter Docker Walkthrough — Compact Demo
# Reads step definitions from demo/walkthrough.yaml (single source of truth).
# Runs inside the distributable Docker image.
#
# Usage:
#   ./demo/walkthrough-docker.sh                  # Build image and run all steps
#   ./demo/walkthrough-docker.sh --image shatter  # Use a pre-built image
#   ./demo/walkthrough-docker.sh --interactive     # Pause after each step
#   ./demo/walkthrough-docker.sh --dry-run        # Print commands without executing

MODE="auto"
DELAY=2
DRY_RUN=false
IMAGE=""
IMAGE_DEFAULT="shatter-walkthrough"

# Color support
if [[ -t 1 ]]; then
    BOLD=$'\033[1m' DIM=$'\033[2m' GREEN=$'\033[32m' CYAN=$'\033[36m'
    YELLOW=$'\033[33m' RED=$'\033[31m' RESET=$'\033[0m'
else
    BOLD="" DIM="" GREEN="" CYAN="" YELLOW="" RED="" RESET=""
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

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
EOF
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --auto)        MODE="auto"; shift ;;
        --interactive) MODE="interactive"; shift ;;
        --delay)       DELAY="$2"; shift 2 ;;
        --dry-run)     DRY_RUN=true; shift ;;
        --image)       IMAGE="$2"; shift 2 ;;
        --help|-h)     usage ;;
        *)             echo "${RED}Unknown option: $1${RESET}"; exit 1 ;;
    esac
done

# Check dependencies
command -v docker &>/dev/null || { echo "${RED}Error: docker not in PATH${RESET}"; exit 1; }
python3 -c "import yaml" 2>/dev/null || { echo "${RED}PyYAML required: pip install pyyaml${RESET}"; exit 1; }

# Build or reuse image
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

ERROR_LOG="$(mktemp "${TMPDIR:-/tmp}/shatter-walkthrough-docker-errors.XXXXXX")"
STEP_ERRORS=0
CACHE_VOL="shatter-walkthrough-cache-$$"

cleanup() {
    rm -f "$ERROR_LOG"
    docker volume rm "$CACHE_VOL" &>/dev/null || true
}
trap cleanup EXIT

if [[ "$DRY_RUN" != true ]]; then
    docker volume create "$CACHE_VOL" >/dev/null
fi

# ─── Helpers ─────────────────────────────────────────────────────────

banner() {
    local num="$1" total="$2" title="$3" desc="$4"
    echo ""
    echo "${BOLD}${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
    echo "${BOLD}  Step ${num}/${total} · ${title}${RESET}"
    echo "${DIM}  ${desc}${RESET}"
    echo "${BOLD}${CYAN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
    echo ""
}

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
# and output tab-delimited step records. Docker paths are NOT remapped —
# examples are mounted at /repo/examples inside the container.
read_manifest() {
    python3 - "$MANIFEST" "$SAMPLE_MANIFEST" <<'PY'
import json
import sys
import yaml

manifest_path, sample_path = sys.argv[1], sys.argv[2]

with open(manifest_path, "r") as fh:
    manifest = yaml.safe_load(fh)

with open(sample_path, "r") as fh:
    samples = json.load(fh)

def resolve_sample_key(dotted_key):
    value = samples
    for part in dotted_key.split("."):
        value = value[part]
    return value

total = sum(1 for s in manifest["steps"] if not s.get("docker", {}).get("skip", False))
step_num = 0

for step in manifest["steps"]:
    docker_meta = step.get("docker", {})
    skip = docker_meta.get("skip", False)
    skip_reason = docker_meta.get("reason", "")

    if skip:
        # Output skip record: num=0 signals skip to the bash loop
        print(f"0\t{total}\t{step['title']}\t{skip_reason}\t", flush=True)
        continue

    step_num += 1
    title = step["title"]
    desc = step["description"]
    command = step["command"]

    # Resolve targets (no path remapping — Docker mounts at /repo/examples)
    targets = []
    if "targets" in step:
        targets = list(resolve_sample_key(step["targets"]))
    elif "targets_first" in step:
        group = resolve_sample_key(step["targets_first"])
        targets = [group[0]]
    elif "targets_literal" in step:
        targets = list(step["targets_literal"])

    args = list(step.get("args", []))

    full_cmd = command
    if args:
        full_cmd += " " + " ".join(args)
    if targets:
        full_cmd += " " + " ".join(targets)

    print(f"{step_num}\t{total}\t{title}\t{desc}\t{full_cmd}", flush=True)
PY
}

echo ""
echo "${BOLD}${GREEN}Shatter Docker Walkthrough${RESET}"
echo "${DIM}Running inside Docker image '${IMAGE}'${RESET}"
if [[ "$DRY_RUN" == true ]]; then
    echo "${YELLOW}(dry-run mode: commands will not be executed)${RESET}"
fi

while IFS=$'\t' read -r num total title desc full_cmd; do
    if [[ "$num" == "0" ]]; then
        # Skipped step
        echo ""
        echo "${DIM}  Skipped · ${title} — ${desc}${RESET}"
        echo ""
        continue
    fi
    CURRENT_STEP="$num"
    banner "$num" "$total" "$title" "$desc"
    # shellcheck disable=SC2086
    run_cmd $full_cmd
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
    echo "${BOLD}${GREEN}Docker walkthrough complete with errors.${RESET}"
    exit 1
else
    echo "${BOLD}${GREEN}Docker walkthrough complete. All steps passed.${RESET}"
fi
