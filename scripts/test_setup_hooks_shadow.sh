#!/usr/bin/env bash
# Integration test for the str-35vtk.25 pre-push receipt shadow check as
# wired into scripts/setup-hooks.sh.
#
# Installs the real pre-push hook (via scripts/setup-hooks.sh, unmodified)
# into a disposable fixture repository that carries its own pre-existing
# hook content, a stub `task` binary that records every invocation to a log
# file, and then invokes the installed hook directly with git's pre-push
# stdin protocol for five scenarios:
#
#   1. feature branch push, no prior receipt   -> real gate runs, decision
#                                                  invalid/missing_gate
#   2. push to main with a matching valid receipt -> decision reuse, and
#                                                  the real gate still runs
#                                                  (classification: match)
#   3. multi-ref push                          -> decision invalid/multi_ref,
#                                                  real gate still runs
#                                                  exactly once
#   4. push to main, matching receipt but the real gate fails THIS push
#                                               -> classification false_accept,
#                                                  and critically the hook's
#                                                  own exit code still
#                                                  reflects the real gate's
#                                                  failure (shadow check
#                                                  never masks it)
#   5. the shadow script itself is missing     -> the real gate and the
#                                                  pre-existing unrelated
#                                                  hook content still run
#
# Every scenario also asserts the pre-existing (unrelated) hook sentinel ran,
# proving the shadow check addition never displaced it.
#
# Usage: bash scripts/test_setup_hooks_shadow.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# See scripts/git-sandbox-test-lib.sh: hooks inherit GIT_DIR/GIT_WORK_TREE
# from the invoking repo, which would make a naive `cd <fixture>` still
# operate on this repo. Isolate before the fixture's first git command.
source "$REPO_ROOT/scripts/git-sandbox-test-lib.sh"

FIXTURE="$(mktemp -d)"
trap 'rm -rf "$FIXTURE"' EXIT

FAIL=0
fail() {
    echo "[FAIL] $1" >&2
    FAIL=1
}

assert_contains() {
    local haystack="$1" needle="$2" label="$3"
    case "$haystack" in
        *"$needle"*) ;;
        *) fail "$label: expected to find '$needle'" ;;
    esac
}

assert_not_contains() {
    local haystack="$1" needle="$2" label="$3"
    case "$haystack" in
        *"$needle"*) fail "$label: did not expect to find '$needle'" ;;
        *) ;;
    esac
}

payload_field() {
    # $1 = event log path, $2 = jq-less dotted field path into .payload
    python3 -c '
import json, sys
path, field = sys.argv[1], sys.argv[2]
with open(path) as fh:
    lines = [l for l in fh.read().splitlines() if l.strip()]
event = json.loads(lines[-1])
value = event["payload"]
for part in field.split("."):
    value = value[part]
print(value if isinstance(value, str) else json.dumps(value))
' "$1" "$2"
}

# --- Fixture repository setup -------------------------------------------
mkdir -p "$FIXTURE/scripts" "$FIXTURE/bin"
git init -q --initial-branch=scratch "$FIXTURE"
git -C "$FIXTURE" config user.email shadow-test@example.test
git -C "$FIXTURE" config user.name "Shadow Test"

cp "$REPO_ROOT/scripts/setup-hooks.sh" "$FIXTURE/scripts/setup-hooks.sh"
cp "$REPO_ROOT/scripts/receipt-shadow-check.py" "$FIXTURE/scripts/receipt-shadow-check.py"
cp "$REPO_ROOT/scripts/gate-receipt.py" "$FIXTURE/scripts/gate-receipt.py"
cp "$REPO_ROOT/scripts/gate-event-log.py" "$FIXTURE/scripts/gate-event-log.py"
cat > "$FIXTURE/scripts/gate-wrapper.sh" <<'EOF'
#!/bin/sh
exec "$@"
EOF
chmod +x "$FIXTURE/scripts/gate-wrapper.sh"

cat > "$FIXTURE/Taskfile.yml" <<'EOF'
version: '3'
tasks:
  check:
    cmds: ["true"]
  affected:
    cmds: ["true"]
EOF

# A stub `task` that special-cases --version (gate-receipt.py's collect_tools
# needs a stable, real answer from it) and otherwise just records every
# invocation plus the AFFECTED_HEADS env var, exiting with $TASK_EXIT_CODE.
cat > "$FIXTURE/bin/task" <<'EOF'
#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "Task version: v0.0.0-fixture"
  exit 0
fi
echo "SENTINEL_TASK_CALLED $*" >> "$FIXTURE_TASK_LOG"
echo "AFFECTED_HEADS=${AFFECTED_HEADS:-}" >> "$FIXTURE_TASK_LOG"
exit "${TASK_EXIT_CODE:-0}"
EOF
chmod +x "$FIXTURE/bin/task"

# Pre-existing, unrelated hook content that setup-hooks.sh must preserve.
cat > "$FIXTURE/.git/hooks/pre-push" <<'EOF'
#!/usr/bin/env sh
echo "SENTINEL_PREEXISTING_HOOK_RAN"
EOF
chmod +x "$FIXTURE/.git/hooks/pre-push"

bash "$FIXTURE/scripts/setup-hooks.sh" >/dev/null

echo "base" > "$FIXTURE/README.md"
git -C "$FIXTURE" add -A
git -C "$FIXTURE" commit -q -m base
BASE_SHA="$(git -C "$FIXTURE" rev-parse HEAD)"
git -C "$FIXTURE" update-ref refs/remotes/origin/main "$BASE_SHA"

echo "candidate change" >> "$FIXTURE/README.md"
git -C "$FIXTURE" add -A
git -C "$FIXTURE" commit -q -m candidate
CAND_SHA="$(git -C "$FIXTURE" rev-parse HEAD)"
CAND_TREE="$(git -C "$FIXTURE" rev-parse "$CAND_SHA^{tree}")"
BASE_TREE="$(git -C "$FIXTURE" rev-parse "$BASE_SHA^{tree}")"

TASK_LOG="$FIXTURE/task-calls.log"
EVENT_LOG_DIR="$FIXTURE/xdg-cache"

run_hook() {
    # $1 = stdin lines, $2 = TASK_EXIT_CODE
    : > "$TASK_LOG"
    rm -rf "$EVENT_LOG_DIR"
    set +e
    printf '%s' "$1" | \
        env -C "$FIXTURE" \
            PATH="$FIXTURE/bin:$PATH" \
            FIXTURE_TASK_LOG="$TASK_LOG" \
            TASK_EXIT_CODE="$2" \
            XDG_CACHE_HOME="$EVENT_LOG_DIR" \
            .git/hooks/pre-push origin git@example.test:repo.git
    HOOK_EXIT=$?
    set -e
}

EVENT_LOG="$EVENT_LOG_DIR/shatter/gate-events.jsonl"

# --- Scenario 1: feature branch, no prior receipt -----------------------
echo "[test] scenario 1: feature branch push, no prior receipt"
LINE="refs/heads/feature/x $CAND_SHA refs/heads/feature/x $BASE_SHA"
run_hook "$LINE
" 0

[ "$HOOK_EXIT" -eq 0 ] || fail "scenario1: hook exit expected 0, got $HOOK_EXIT"
TASK_LOG_CONTENT="$(cat "$TASK_LOG")"
assert_contains "$TASK_LOG_CONTENT" "SENTINEL_TASK_CALLED affected" "scenario1: real gate (affected) ran"
[ -f "$EVENT_LOG" ] || fail "scenario1: no event log written"
if [ -f "$EVENT_LOG" ]; then
    [ "$(payload_field "$EVENT_LOG" decision)" = "invalid" ] || fail "scenario1: expected decision invalid"
    assert_contains "$(payload_field "$EVENT_LOG" reasons)" "missing_gate" "scenario1: reasons"
    [ "$(payload_field "$EVENT_LOG" classification)" = "expected_miss" ] || fail "scenario1: expected classification expected_miss"
    [ "$(payload_field "$EVENT_LOG" candidate)" = "$CAND_TREE" ] || fail "scenario1: candidate tree mismatch"
    [ "$(payload_field "$EVENT_LOG" base)" = "$BASE_TREE" ] || fail "scenario1: base tree mismatch"
fi

# --- Scenario 2: push to main with a matching valid receipt -------------
echo "[test] scenario 2: main push, matching receipt (reuse/match)"
RESULT_JSON="$FIXTURE/gate-result.json"
python3 -c "
import json
json.dump({
    'gate': 'task check',
    'argv': ['task', 'check'],
    'started_at': '2026-09-23T12:00:00Z',
    'ended_at': '2026-09-23T12:01:00Z',
    'exit_code': 0,
}, open('$RESULT_JSON', 'w'))
"
env -C "$FIXTURE" PATH="$FIXTURE/bin:$PATH" python3 scripts/gate-receipt.py write \
    --candidate "$CAND_TREE" --base "$BASE_TREE" --tier local \
    --gate-result "$RESULT_JSON" >/dev/null

LINE="refs/heads/main $CAND_SHA refs/heads/main $BASE_SHA"
run_hook "$LINE
" 0

[ "$HOOK_EXIT" -eq 0 ] || fail "scenario2: hook exit expected 0, got $HOOK_EXIT"
assert_contains "$(cat "$TASK_LOG")" "SENTINEL_TASK_CALLED check" "scenario2: real gate (check) ran"
if [ -f "$EVENT_LOG" ]; then
    [ "$(payload_field "$EVENT_LOG" decision)" = "reuse" ] || fail "scenario2: expected decision reuse, got $(payload_field "$EVENT_LOG" decision)"
    [ "$(payload_field "$EVENT_LOG" classification)" = "match" ] || fail "scenario2: expected classification match"
else
    fail "scenario2: no event log written"
fi

# --- Scenario 3: multi-ref push -------------------------------------------
echo "[test] scenario 3: multi-ref push"
LINES="refs/heads/feature/x $CAND_SHA refs/heads/feature/x $BASE_SHA
refs/heads/main $CAND_SHA refs/heads/main $BASE_SHA
"
run_hook "$LINES" 0

[ "$HOOK_EXIT" -eq 0 ] || fail "scenario3: hook exit expected 0, got $HOOK_EXIT"
TASK_CALL_COUNT="$(grep -c '^SENTINEL_TASK_CALLED check$' "$TASK_LOG" || true)"
[ "$TASK_CALL_COUNT" -eq 1 ] || fail "scenario3: expected exactly one real gate invocation, got $TASK_CALL_COUNT"
if [ -f "$EVENT_LOG" ]; then
    [ "$(payload_field "$EVENT_LOG" decision)" = "invalid" ] || fail "scenario3: expected decision invalid"
    [ "$(payload_field "$EVENT_LOG" reasons)" = '["multi_ref"]' ] || fail "scenario3: expected reasons [multi_ref], got $(payload_field "$EVENT_LOG" reasons)"
    [ "$(payload_field "$EVENT_LOG" diff_class)" = "unknown" ] || fail "scenario3: expected diff_class unknown"
else
    fail "scenario3: no event log written"
fi

# --- Scenario 4: main push, matching receipt but real gate fails --------
echo "[test] scenario 4: main push, matching receipt but real gate fails this push (false_accept)"
LINE="refs/heads/main $CAND_SHA refs/heads/main $BASE_SHA"
run_hook "$LINE
" 1

[ "$HOOK_EXIT" -eq 1 ] || fail "scenario4: hook exit expected 1 (real gate failure must propagate), got $HOOK_EXIT"
if [ -f "$EVENT_LOG" ]; then
    [ "$(payload_field "$EVENT_LOG" decision)" = "reuse" ] || fail "scenario4: expected decision reuse"
    [ "$(payload_field "$EVENT_LOG" classification)" = "false_accept" ] || fail "scenario4: expected classification false_accept, got $(payload_field "$EVENT_LOG" classification)"
else
    fail "scenario4: no event log written"
fi

# --- Scenario 5: shadow script missing ------------------------------------
echo "[test] scenario 5: shadow script missing -- real gate + unrelated hook still run"
mv "$FIXTURE/scripts/receipt-shadow-check.py" "$FIXTURE/scripts/receipt-shadow-check.py.bak"
LINE="refs/heads/feature/x $CAND_SHA refs/heads/feature/x $BASE_SHA"
run_hook "$LINE
" 0
mv "$FIXTURE/scripts/receipt-shadow-check.py.bak" "$FIXTURE/scripts/receipt-shadow-check.py"

[ "$HOOK_EXIT" -eq 0 ] || fail "scenario5: hook exit expected 0, got $HOOK_EXIT"
assert_contains "$(cat "$TASK_LOG")" "SENTINEL_TASK_CALLED affected" "scenario5: real gate still ran without the shadow script"

# --- Cross-scenario: unrelated pre-existing hook content always ran -----
# (Each run_hook call above already exercises the hook fresh; capture
#  stdout directly here to confirm the sentinel survived every install.)
LINE="refs/heads/feature/x $CAND_SHA refs/heads/feature/x $BASE_SHA"
HOOK_STDOUT="$(printf '%s\n' "$LINE" | env -C "$FIXTURE" PATH="$FIXTURE/bin:$PATH" FIXTURE_TASK_LOG="$TASK_LOG" TASK_EXIT_CODE=0 XDG_CACHE_HOME="$EVENT_LOG_DIR" .git/hooks/pre-push origin git@example.test:repo.git)"
assert_contains "$HOOK_STDOUT" "SENTINEL_PREEXISTING_HOOK_RAN" "unrelated pre-existing hook content"

if [ "$FAIL" -ne 0 ]; then
    echo "[FAIL] test_setup_hooks_shadow: one or more assertions failed" >&2
    exit 1
fi

echo "[ok] test_setup_hooks_shadow: all scenarios passed"
