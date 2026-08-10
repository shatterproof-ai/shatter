#!/usr/bin/env bash
# land-work project verifier (see .agent-plugins/bento/bento/land-work/verifier.json).
# Runs the same gates ci.yml uses to gate merges to main and reports the
# schema_version=1 result land-work-run-verifier.py expects on its final
# stdout line.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

checks_json=()
overall_status="passed"

run_check() {
    local name="$1"
    shift
    if "$@" >/dev/null 2>&1; then
        checks_json+=("{\"name\":\"${name}\",\"status\":\"passed\"}")
    else
        checks_json+=("{\"name\":\"${name}\",\"status\":\"failed\"}")
        overall_status="failed"
    fi
}

run_check "task test-standard" task test-standard
run_check "task parity" task parity
run_check "task conformance" task conformance

joined=$(IFS=,; echo "${checks_json[*]}")
echo "{\"schema_version\":1,\"status\":\"${overall_status}\",\"selected_checks\":[${joined}]}"

if [ "$overall_status" != "passed" ]; then
    exit 1
fi
