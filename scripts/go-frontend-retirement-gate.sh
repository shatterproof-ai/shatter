#!/usr/bin/env bash
# go-frontend-retirement-gate.sh
#
# CI gate for the Go-frontend retirement (str-hy9b expedition, J1).
# Exercises three fixture classes against the new Go-frontend path
# (shatter-go/main.go → planner → launcher → builder) and emits a
# structured PASS/FAIL report. Exit 0 if every fixture passes,
# 1 otherwise.

set -u
set -o pipefail

SCRIPT_NAME="$(basename "$0")"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

VERBOSE=0
EXAMPLES_DIR="$REPO_ROOT/examples/go"
HTTP_FIXTURE_DIR="$REPO_ROOT/shatter-go/protocol/testdata"

usage() {
    cat <<EOF
Usage: $SCRIPT_NAME [-h] [-v]

Retirement-gate probe for the Go frontend. Runs three classes of
fixture through the post-J2 path (no legacy instrument.Execute*) and
emits a structured PASS/FAIL summary.

  Class 1 — gauntlet (in-tree examples/go fixtures: receiver, package, multi-file)
  Class 2 — D7 spike fixture (examples/go/internal-method, module example.com/spike)
  Class 3 — net/http handler via the G1 adapter (shatter-go/protocol nethttp tests)

Options:
  -h, --help     Show this help and exit.
  -v, --verbose  Echo every command and stream tool output verbatim.

Environment:
  SHATTER_BIN    Path to a prebuilt shatter binary. If unset the script
                 looks for ./target/debug/shatter and otherwise builds
                 it with: cargo build --bin shatter.
EOF
}

while (( $# )); do
    case "$1" in
        -h|--help) usage; exit 0 ;;
        -v|--verbose) VERBOSE=1; shift ;;
        *) echo "unknown argument: $1" >&2; usage >&2; exit 2 ;;
    esac
done

# ─── Logging helpers ─────────────────────────────────────────────────
log()    { printf '[gate] %s\n' "$*"; }
vlog()   { (( VERBOSE )) && printf '[gate] %s\n' "$*" || true; }
section() { printf '\n=== %s ===\n' "$*"; }

run_capture() {
    # Run a command, capture stdout+stderr to a temp file.
    # Echo the file path on success; print contents on failure.
    local tag="$1"; shift
    local out
    out="$(mktemp -t "shatter-gate-${tag}.XXXXXX.log")"
    if (( VERBOSE )); then
        printf '[gate] $ %s\n' "$*"
        if "$@" 2>&1 | tee "$out"; then
            return 0
        else
            return $?
        fi
    else
        if "$@" >"$out" 2>&1; then
            rm -f "$out"
            return 0
        else
            local rc=$?
            echo "--- output (${tag}) ---"
            cat "$out"
            echo "--- end output ---"
            rm -f "$out"
            return $rc
        fi
    fi
}

# ─── Locate shatter binary ───────────────────────────────────────────
locate_shatter() {
    if [[ -n "${SHATTER_BIN:-}" && -x "$SHATTER_BIN" ]]; then
        SHATTER="$SHATTER_BIN"
    elif [[ -x "$REPO_ROOT/target/debug/shatter" ]]; then
        SHATTER="$REPO_ROOT/target/debug/shatter"
    else
        log "shatter binary not found; building (cargo build --bin shatter)"
        if ! (cd "$REPO_ROOT" && cargo build --bin shatter); then
            log "FATAL: cargo build failed"
            exit 1
        fi
        SHATTER="$REPO_ROOT/target/debug/shatter"
    fi
    vlog "using shatter binary: $SHATTER"
}

# ─── Result accounting ───────────────────────────────────────────────
declare -a RESULTS=()  # entries: "PASS|FAIL<TAB>class<TAB>name<TAB>detail"
record() {
    local status="$1" class="$2" name="$3" detail="${4:-}"
    RESULTS+=("${status}"$'\t'"${class}"$'\t'"${name}"$'\t'"${detail}")
}

# ─── Probes ──────────────────────────────────────────────────────────
TIMEOUT_ANALYZE_SECS=30
TIMEOUT_EXPLORE_SECS=45
EXPLORE_MAX_ITER=4

# Class 1: gauntlet — analyze each in-tree Go example fixture.
probe_gauntlet() {
    section "Class 1: examples/go gauntlet (analyze)"
    local fixture_files=(
        "$EXAMPLES_DIR/internal-method/api.go"
        "$EXAMPLES_DIR/multi-file-service/service.go"
        "$EXAMPLES_DIR/service-method/svc.go"
    )
    for fixture in "${fixture_files[@]}"; do
        local label="${fixture#$REPO_ROOT/}"
        if [[ ! -f "$fixture" ]]; then
            log "MISSING: $label"
            record FAIL gauntlet "$label" "fixture file missing"
            continue
        fi
        log "analyze: $label"
        if run_capture "analyze" timeout "$TIMEOUT_ANALYZE_SECS" \
            "$SHATTER" explore --analyze-only "$fixture"; then
            record PASS gauntlet "$label" ""
        else
            record FAIL gauntlet "$label" "analyze exit=$?"
        fi
    done
}

# Class 2: D7 spike fixture — explore the internal-method module
# (module example.com/spike) end-to-end through the new launcher path.
probe_d7_spike() {
    section "Class 2: D7 spike fixture (internal-method)"
    local spike="$EXAMPLES_DIR/internal-method/api.go"
    local label="examples/go/internal-method/api.go (D7 spike)"
    if [[ ! -f "$spike" ]]; then
        record FAIL d7_spike "$label" "fixture file missing"
        return
    fi
    log "explore: $label"
    if run_capture "spike" timeout "$TIMEOUT_EXPLORE_SECS" \
        "$SHATTER" explore \
            --max-iterations "$EXPLORE_MAX_ITER" \
            --timeout-explore 20 \
            "$spike"; then
        record PASS d7_spike "$label" ""
    else
        record FAIL d7_spike "$label" "explore exit=$?"
    fi
}

# Class 3: net/http handler through the G1 adapter.
# The protocol-level recognizer + adapter dispatch is exercised by the
# Go unit + integration tests in shatter-go/protocol; this is the
# canonical harness for the G1 adapter on the post-retirement path
# (no legacy http_harness.go callsites). Building shatter-go itself
# also confirms the new path compiles without the deleted instrument
# package.
probe_nethttp_adapter() {
    section "Class 3: net/http handler via G1 adapter"
    local label="shatter-go/protocol nethttp_recognizer + adapter"
    if ! command -v go >/dev/null 2>&1; then
        record FAIL nethttp_adapter "$label" "go toolchain unavailable"
        return
    fi
    log "build: shatter-go/..."
    if ! run_capture "go-build" go -C "$REPO_ROOT/shatter-go" build ./...; then
        record FAIL nethttp_adapter "$label" "shatter-go build failed"
        return
    fi
    local handler_fixture="$HTTP_FIXTURE_DIR/http_handler.go"
    if [[ ! -f "$handler_fixture" ]]; then
        record FAIL nethttp_adapter "$label" "http_handler.go fixture missing"
        return
    fi
    log "go test: nethttp_recognizer + adapter"
    if run_capture "go-test-nethttp" \
        go -C "$REPO_ROOT/shatter-go" test ./protocol/ -run 'NetHTTP|HTTPHandler|nethttp' -count=1; then
        record PASS nethttp_adapter "$label" ""
    else
        record FAIL nethttp_adapter "$label" "go test exit=$?"
    fi
}

# ─── Report ──────────────────────────────────────────────────────────
emit_report() {
    section "Retirement-gate report"
    local pass_count=0 fail_count=0
    declare -A class_pass class_fail
    printf '%-6s  %-18s  %s\n' "STATUS" "CLASS" "FIXTURE"
    printf '%-6s  %-18s  %s\n' "------" "-----" "-------"
    local entry
    for entry in "${RESULTS[@]}"; do
        local status class name detail
        IFS=$'\t' read -r status class name detail <<<"$entry"
        printf '%-6s  %-18s  %s\n' "$status" "$class" "$name"
        if [[ -n "$detail" ]]; then
            printf '%-6s  %-18s    detail: %s\n' "" "" "$detail"
        fi
        if [[ "$status" == "PASS" ]]; then
            (( ++pass_count ))
            class_pass[$class]=$(( ${class_pass[$class]:-0} + 1 ))
        else
            (( ++fail_count ))
            class_fail[$class]=$(( ${class_fail[$class]:-0} + 1 ))
        fi
    done
    echo
    echo "Per-class counts:"
    local class
    for class in gauntlet d7_spike nethttp_adapter; do
        printf '  %-18s  pass=%-3d fail=%-3d\n' \
            "$class" "${class_pass[$class]:-0}" "${class_fail[$class]:-0}"
    done
    echo
    printf 'TOTAL:  pass=%d  fail=%d\n' "$pass_count" "$fail_count"
    if (( fail_count > 0 )); then
        echo "RESULT: FAIL"
        return 1
    fi
    echo "RESULT: PASS"
    return 0
}

# ─── Main ────────────────────────────────────────────────────────────
locate_shatter
probe_gauntlet
probe_d7_spike
probe_nethttp_adapter
if emit_report; then
    exit 0
else
    exit 1
fi
