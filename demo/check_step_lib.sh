#!/usr/bin/env bash
# Shared per-step output check for the demo gates (str-49drv.149): sourced by
# demo/gauntlet.sh, demo/walkthrough.sh, demo/gauntlet-docker.sh and
# demo/walkthrough-docker.sh so every gate screens step output with the one
# checker, demo/gauntlet_check_output.py, and the one allowlist.
#
# Callers provide ERROR_LOG and STEP_ERRORS (both in the caller's scope).
#
#   scan_step_setup HOST_DIR CMD_DIR shatter-args...
#       For `scan` steps, sets SCAN_JSON_FILE (host path of the JSON report),
#       SCAN_JSON_EXTRA_ARGS (`-o <CMD_DIR>/<name>`, appended to the command so
#       the run also writes `--format json` output; CMD_DIR is the same
#       directory as seen from where the command runs, e.g. inside a container)
#       and SCAN_CHECK_ARGS (flags for check_step_output):
#         --dry-run       executes nothing -> `--no-scan-json` (logged, skipped)
#         --changed/--since may select no files (no report) -> same
#         --timeout-total bounds the run by design -> `--expect-interrupted`
#   check_step_output OUTPUT_FILE STEP
#       Runs the checker over the captured output (+ scan JSON, when set up),
#       appends anything flagged to ERROR_LOG, and removes the scan JSON.

_CHECK_STEP_LIB_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

scan_step_setup() {
    SCAN_JSON_FILE=""
    SCAN_JSON_EXTRA_ARGS=()
    SCAN_CHECK_ARGS=()
    local host_dir="$1" cmd_dir="$2"
    shift 2
    [[ "${1:-}" == "scan" ]] || return 0
    local arg dry_run=false bounded=false selects=false
    for arg in "$@"; do
        case "$arg" in
            --dry-run) dry_run=true ;;
            --changed|--since|--since=*) selects=true ;;
            --timeout-total|--timeout-total=*) bounded=true ;;
        esac
    done
    if [[ "$dry_run" == true ]]; then
        SCAN_CHECK_ARGS=(--no-scan-json "--dry-run executes nothing, so no scan JSON is produced")
        return 0
    fi
    if [[ "$selects" == true ]]; then
        SCAN_CHECK_ARGS=(--no-scan-json "--changed/--since may select no files, in which case scan writes no report")
        return 0
    fi
    local name="shatter-scan-json.$$.${RANDOM}${RANDOM}.json"
    SCAN_JSON_FILE="$host_dir/$name"
    SCAN_JSON_EXTRA_ARGS=(-o "$cmd_dir/$name")
    SCAN_CHECK_ARGS=(--scan-json "$SCAN_JSON_FILE")
    if [[ "$bounded" == true ]]; then
        SCAN_CHECK_ARGS+=(--expect-interrupted "--timeout-total bounds the scan by design")
    fi
}

check_step_output() {
    local output="$1" step="$2"
    local helper="${_CHECK_STEP_LIB_DIR}/gauntlet_check_output.py"
    local allowlist="${_CHECK_STEP_LIB_DIR}/gauntlet-scan-allowlist.yaml"
    if [[ ! -f "$helper" || ! -f "$allowlist" ]]; then
        echo "  Step ${step}: output checker or allowlist missing (${helper})" >> "$ERROR_LOG"
        STEP_ERRORS=$((STEP_ERRORS + 1))
        return 0
    fi
    local check_out
    check_out="$(mktemp)"
    if python3 "$helper" \
        --allowlist "$allowlist" \
        --output "$output" \
        --step "$step" \
        ${SCAN_CHECK_ARGS[@]+"${SCAN_CHECK_ARGS[@]}"} >"$check_out" 2>&1; then
        [[ -s "$check_out" ]] && cat "$check_out"
    else
        cat "$check_out" >> "$ERROR_LOG"
        STEP_ERRORS=$((STEP_ERRORS + 1))
    fi
    rm -f "$check_out"
    [[ -n "${SCAN_JSON_FILE:-}" ]] && rm -f "$SCAN_JSON_FILE"
    return 0
}
