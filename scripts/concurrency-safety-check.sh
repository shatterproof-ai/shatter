#!/usr/bin/env bash

set -uo pipefail

CHECKOUT="${1:-$(git rev-parse --show-toplevel)}"
RUN_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/str-35vtk4-acceptance.XXXXXX")"
echo "run_root=$RUN_ROOT"

run_case() {
    local name="$1"
    shift
    (
        local started ended rc
        started="$(date +%s)"
        "$@" > "$RUN_ROOT/$name.log" 2>&1
        rc=$?
        ended="$(date +%s)"
        printf 'name=%s rc=%s wall_seconds=%s\n' "$name" "$rc" "$((ended - started))" \
            > "$RUN_ROOT/$name.result"
        exit "$rc"
    ) &
    PIDS+=("$!")
    NAMES+=("$name")
}

cd "$CHECKOUT" || exit 2
PIDS=()
NAMES=()
run_case smoke-1 task --force smoke
run_case smoke-2 task --force smoke
run_case gauntlet-1 env \
    SHATTER_DEMO_CACHE="$RUN_ROOT/demo-cache" \
    SHATTER_GAUNTLET_EVIDENCE_DIR="$RUN_ROOT/gauntlet-1-evidence" \
    bash demo/gauntlet.sh --auto --delay 0 --step-timeout 300
run_case gauntlet-2 env \
    SHATTER_DEMO_CACHE="$RUN_ROOT/demo-cache" \
    SHATTER_GAUNTLET_EVIDENCE_DIR="$RUN_ROOT/gauntlet-2-evidence" \
    bash demo/gauntlet.sh --auto --delay 0 --step-timeout 300

overall=0
for index in "${!PIDS[@]}"; do
    if ! wait "${PIDS[$index]}"; then
        echo "[FAIL] ${NAMES[$index]} failed; see $RUN_ROOT/${NAMES[$index]}.log" >&2
        overall=1
    fi
done

python3 - "$RUN_ROOT" <<'PY'
import json
import sys
from pathlib import Path

root = Path(sys.argv[1])
evidence_dirs = [root / "gauntlet-1-evidence", root / "gauntlet-2-evidence"]
run_paths = []
for evidence_dir in evidence_dirs:
    manifest = {}
    for line in (evidence_dir / "run-paths.txt").read_text(encoding="utf-8").splitlines():
        key, value = line.split("=", 1)
        manifest[key] = value
    run_paths.append(manifest)

for index, evidence_dir in enumerate(evidence_dirs):
    json_files = sorted(evidence_dir.glob("*.json"))
    markdown_files = sorted(evidence_dir.glob("*.md"))
    if len(json_files) < 3 or not markdown_files:
        raise SystemExit(
            f"{evidence_dir}: expected at least 3 JSON specs/observations and one Markdown report"
        )
    for path in json_files:
        json.loads(path.read_text(encoding="utf-8"))
    foreign = run_paths[1 - index]
    for path in [*json_files, *markdown_files, evidence_dir / "run-paths.txt"]:
        text = path.read_text(encoding="utf-8")
        for foreign_path in foreign.values():
            if foreign_path and foreign_path in text:
                raise SystemExit(f"{path}: references foreign run path {foreign_path}")

print("[ok] gauntlet JSON/Markdown evidence parses and contains no foreign run paths")
PY
validation_rc=$?
if [[ "$validation_rc" -ne 0 ]]; then
    overall=1
fi

cat "$RUN_ROOT"/*.result | tee "$RUN_ROOT/summary.txt"
if [[ "$overall" -eq 0 ]]; then
    echo "[ok] two smoke and two gauntlet runs completed without cross-contamination" \
        | tee -a "$RUN_ROOT/summary.txt"
fi
exit "$overall"
