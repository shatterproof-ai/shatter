#!/usr/bin/env bash
# Regression test: A4 — Empty-report / failed-instrumentation report regression.
#
# Runs `shatter explore` on the internal-method fixture, which is a Go package
# whose go.mod is intentionally out of sync (go mod tidy not run). The frontend
# cannot instrument the package, so the explore should:
#
#   1. Produce a non-empty markdown report.
#   2. Include the target function's name in the report.
#   3. Report an outcome status of one of: build_failed, unsupported, runtime_failed.
#
# This test guards against a regression where a failed instrumentation/build
# silently emitted an empty or placeholder report rather than an honest failure
# heading (as fixed by str-hy9b.A3).
#
# Usage: bash scripts/test_empty_report_regression.sh [-v|--verbose]

set -euo pipefail

VERBOSE=0
for arg in "$@"; do
    case "$arg" in
        -v|--verbose) VERBOSE=1 ;;
        -h|--help)
            sed -n '2,20p' "$0" | sed 's/^# *//'
            exit 0
            ;;
    esac
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FIXTURE="$REPO_ROOT/examples/go/internal-method/internal/svc/svc.go"
FUNCTION_NAME="DoIt"
TARGET="$FIXTURE:$FUNCTION_NAME"

# Prefer the pre-built debug binary; fall back to cargo run.
if [[ -x "$REPO_ROOT/target/debug/shatter" ]]; then
    SHATTER_CMD="$REPO_ROOT/target/debug/shatter"
else
    SHATTER_CMD="cargo run -p shatter-cli --"
fi

REPORT="$(mktemp /tmp/shatter-a4-regression-XXXXXX.md)"
trap 'rm -f "$REPORT"' EXIT

if [[ "$VERBOSE" -eq 1 ]]; then
    echo "[a4] fixture : $TARGET"
    echo "[a4] report  : $REPORT"
    echo "[a4] binary  : $SHATTER_CMD"
fi

# Run explore; the exit code is 0 even when exploration fails.
$SHATTER_CMD explore \
    --max-iterations 1 \
    --timeout-explore 15 \
    --output "$REPORT" \
    "$TARGET" 2>&1 | { [[ "$VERBOSE" -eq 1 ]] && cat || cat > /dev/null; }

# --- Assertion 1: report is non-empty ---
if [[ ! -s "$REPORT" ]]; then
    echo "[FAIL] a4: report file is empty — non-empty output expected for failed instrumentation" >&2
    exit 1
fi

REPORT_CONTENT="$(cat "$REPORT")"

if [[ "$VERBOSE" -eq 1 ]]; then
    echo "[a4] report contents:"
    echo "$REPORT_CONTENT"
fi

# --- Assertion 2: report contains the function's name ---
if ! grep -q "$FUNCTION_NAME" "$REPORT"; then
    echo "[FAIL] a4: report does not contain function name '$FUNCTION_NAME'" >&2
    echo "       report contents:" >&2
    cat "$REPORT" >&2
    exit 1
fi

# --- Assertion 3: report contains one of the expected failure status labels ---
if ! grep -qE '`(build_failed|unsupported|runtime_failed)`' "$REPORT"; then
    echo "[FAIL] a4: report does not contain one of {build_failed, unsupported, runtime_failed}" >&2
    echo "       report contents:" >&2
    cat "$REPORT" >&2
    exit 1
fi

echo "[ok] a4: empty-report regression — report non-empty, contains '$FUNCTION_NAME', outcome is a failure status"
