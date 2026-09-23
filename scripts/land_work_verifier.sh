#!/usr/bin/env bash
# land-work project verifier (see .agent-plugins/bento/bento/land-work/verifier.json).
#
# Runs exactly one `task check` -- the same gate ci.yml uses to gate merges
# to main -- against the exact candidate tree, and reports the
# schema_version=1 result land-work-run-verifier.py expects on its final
# stdout line.
#
# On a passing gate, also writes a local-tier gate receipt (via
# scripts/gate-receipt.py) recording the exact candidate/base trees and the
# gate's argv/timestamps/exit_code, so later landings on the same tree/base
# pair can trust cached evidence (str-35vtk.24). A receipt-write failure
# fails verification but does not discard the gate's own stdout/stderr: they
# are forwarded through this script's own stdout/stderr before the final
# result line. That is all the existing Bento land-work-run-verifier.py
# collector needs -- it already captures this whole process's stdout+stderr
# to its own verifier.log and parses only the final stdout line as the
# result JSON -- so no second log protocol is introduced here.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fail() {
    echo "land_work_verifier: $1" >&2
    echo "{\"schema_version\":1,\"status\":\"failed\",\"selected_checks\":[]}"
    exit 1
}

candidate_tree="$(git rev-parse HEAD^{tree} 2>/dev/null)" \
    || fail "cannot resolve candidate tree (git rev-parse HEAD^{tree})"

# A missing origin/main (never fetched) must fail before the gate ever runs
# -- there is no base to diff against otherwise. This only checks that the
# ref resolves locally; it does not detect a stale-but-present origin/main.
git rev-parse --verify -q 'origin/main^{commit}' >/dev/null 2>&1 \
    || fail "origin/main is missing or not fetched; fetch origin main before verifying"

merge_base="$(git merge-base HEAD origin/main 2>/dev/null)" \
    || fail "cannot compute merge-base of HEAD and origin/main"
base_tree="$(git rev-parse "${merge_base}^{tree}" 2>/dev/null)" \
    || fail "cannot resolve base tree (git rev-parse <merge-base>^{tree})"

gate_stdout="$(mktemp)"
gate_stderr="$(mktemp)"
gate_result="$(mktemp)"
cleanup() { rm -f "$gate_stdout" "$gate_stderr" "$gate_result"; }
trap cleanup EXIT

started_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
task check >"$gate_stdout" 2>"$gate_stderr"
exit_code=$?
ended_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

# Forward the gate's own diagnostics unconditionally -- the Bento
# collector's verifier.log must retain them even when a later step (the
# receipt write) is what ultimately fails.
cat "$gate_stdout"
cat "$gate_stderr" >&2

if [ "$exit_code" -ne 0 ]; then
    echo "{\"schema_version\":1,\"status\":\"failed\",\"selected_checks\":[{\"name\":\"task check\",\"status\":\"failed\"}]}"
    exit 1
fi

python3 - "$gate_result" "$started_at" "$ended_at" <<'PY'
import json
import sys

path, started_at, ended_at = sys.argv[1:4]
with open(path, "w", encoding="utf-8") as handle:
    json.dump(
        {
            "gate": "task check",
            "argv": ["task", "check"],
            "started_at": started_at,
            "ended_at": ended_at,
            "exit_code": 0,
        },
        handle,
    )
PY

if ! receipt_error="$(python3 "$repo_root/scripts/gate-receipt.py" write \
    --candidate "$candidate_tree" \
    --base "$base_tree" \
    --tier local \
    --gate-result "$gate_result" 2>&1 >/dev/null)"; then
    echo "land_work_verifier: gate-receipt write failed: ${receipt_error}" >&2
    echo "{\"schema_version\":1,\"status\":\"failed\",\"selected_checks\":[{\"name\":\"task check\",\"status\":\"passed\"},{\"name\":\"gate-receipt\",\"status\":\"failed\"}]}"
    exit 1
fi

echo "{\"schema_version\":1,\"status\":\"passed\",\"selected_checks\":[{\"name\":\"task check\",\"status\":\"passed\"},{\"name\":\"gate-receipt\",\"status\":\"passed\"}]}"
