#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SETUP_HOOKS="$REPO_ROOT/scripts/setup-hooks.sh"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

TEST_REPO="$SCRATCH/repo"
mkdir -p "$TEST_REPO/scripts"
git init -q "$TEST_REPO"
cp "$SETUP_HOOKS" "$TEST_REPO/scripts/setup-hooks.sh"
chmod +x "$TEST_REPO/scripts/setup-hooks.sh"

HOOKS=(pre-commit post-merge pre-push post-checkout prepare-commit-msg)
for hook in "${HOOKS[@]}"; do
    hook_file="$TEST_REPO/.git/hooks/$hook"
    cat > "$hook_file" <<EOF
#!/usr/bin/env sh
printf '%s=%s\n' '$hook' "\${BEADS_HOOK_TIMEOUT:-unset}" >> "\$HOOK_OUTPUT"
EOF
    chmod +x "$hook_file"
done

(cd "$TEST_REPO" && scripts/setup-hooks.sh)
(cd "$TEST_REPO" && scripts/setup-hooks.sh --check)

QUALITY_VERSION_MARKER="# SHATTER QUALITY TEMPLATE VERSION: 2"
PRE_PUSH_HOOK="$TEST_REPO/.git/hooks/pre-push"
grep -qF "$QUALITY_VERSION_MARKER" "$PRE_PUSH_HOOK"
sed -i "s/$QUALITY_VERSION_MARKER/# SHATTER QUALITY TEMPLATE VERSION: 1/" "$PRE_PUSH_HOOK"
set +e
(cd "$TEST_REPO" && scripts/setup-hooks.sh --check) >/dev/null 2>&1
STALE_CHECK_RC=$?
set -e
if [[ "$STALE_CHECK_RC" -eq 0 ]]; then
    echo "[FAIL] --check accepted a stale pre-push quality template" >&2
    exit 1
fi
(cd "$TEST_REPO" && scripts/setup-hooks.sh)
grep -qF "$QUALITY_VERSION_MARKER" "$PRE_PUSH_HOOK"
if [[ "$(grep -cF '# --- BEGIN SHATTER QUALITY ---' "$PRE_PUSH_HOOK")" -ne 1 ]]; then
    echo "[FAIL] stale quality template refresh left duplicate sections" >&2
    exit 1
fi

BEFORE="$SCRATCH/before.sha256"
AFTER="$SCRATCH/after.sha256"
sha256sum "${HOOKS[@]/#/$TEST_REPO/.git/hooks/}" > "$BEFORE"
(cd "$TEST_REPO" && scripts/setup-hooks.sh)
sha256sum "${HOOKS[@]/#/$TEST_REPO/.git/hooks/}" > "$AFTER"
cmp "$BEFORE" "$AFTER"

OUTPUT="$SCRATCH/hooks.out"
for hook in "${HOOKS[@]}"; do
    (cd "$TEST_REPO" && HOOK_OUTPUT="$OUTPUT" ".git/hooks/$hook" </dev/null)
done
for hook in "${HOOKS[@]}"; do
    grep -qx "$hook=30" "$OUTPUT"
done

: > "$OUTPUT"
(cd "$TEST_REPO" && HOOK_OUTPUT="$OUTPUT" BEADS_HOOK_TIMEOUT=7 ".git/hooks/post-checkout")
grep -qx 'post-checkout=7' "$OUTPUT"

for hook in "${HOOKS[@]}"; do
    hook_file="$TEST_REPO/.git/hooks/$hook"
    env_line="$(grep -n 'export BEADS_HOOK_TIMEOUT=' "$hook_file" | cut -d: -f1)"
    body_line="$(grep -n "printf '%s=%s" "$hook_file" | cut -d: -f1)"
    if [[ -z "$env_line" || "$env_line" -ge "$body_line" ]]; then
        echo "[FAIL] $hook timeout environment must precede existing hook content" >&2
        exit 1
    fi
done

echo "[ok] setup-hooks installs an ordered, idempotent 30s Beads timeout for every hook"

### Pre-push ref classification ###
# Table: local-feature->remote-main, other heads, tags, deletion, mixed,
# empty/blank stdin, malformed input, and exact `task` invocation counts.

: > "$TEST_REPO/Taskfile.yml"

TASK_BIN_DIR="$SCRATCH/bin"
mkdir -p "$TASK_BIN_DIR"
TASK_LOG="$SCRATCH/task.log"
HEADS_LOG="$SCRATCH/heads.log"
cat > "$TASK_BIN_DIR/task" <<'EOF'
#!/usr/bin/env sh
printf '%s\n' "$1" >> "$TASK_LOG"
printf '%s\n' "${AFFECTED_HEADS:-}" >> "$HEADS_LOG"
exit 0
EOF
chmod +x "$TASK_BIN_DIR/task"

SHA_A=$(printf 'a%.0s' {1..40})
SHA_B=$(printf 'b%.0s' {1..40})
ZERO40=$(printf '0%.0s' {1..40})
SHA256_A=$(printf 'a%.0s' {1..64})
NONHEX40=$(printf 'g%.0s' {1..40})
SHA_C=$(printf 'c%.0s' {1..40})

RC=0
run_prepush() {
    # $1 = stdin content, $2 = extra env assignment (e.g. SHATTER_FULL_PUSH=1)
    : > "$TASK_LOG"
    : > "$HEADS_LOG"
    set +e
    printf '%s' "$1" | env PATH="$TASK_BIN_DIR:$PATH" TASK_LOG="$TASK_LOG" HEADS_LOG="$HEADS_LOG" ${2:-} \
        bash -c 'cd "$0" && exec "$1"' "$TEST_REPO" "$PRE_PUSH_HOOK" \
        > "$SCRATCH/prepush.out" 2>&1
    RC=$?
    set -e
}

assert_rc() {
    # $1 = expected rc, $2 = test name
    if [[ "$RC" -ne "$1" ]]; then
        echo "[FAIL] $2: expected exit $1, got $RC" >&2
        cat "$SCRATCH/prepush.out" >&2
        exit 1
    fi
}

assert_task_log() {
    # $1 = expected task.log content (may be empty), $2 = test name
    if [[ "$(cat "$TASK_LOG")" != "$1" ]]; then
        echo "[FAIL] $2: expected task.log '$1', got '$(cat "$TASK_LOG")'" >&2
        exit 1
    fi
}

assert_heads_log() {
    # $1 = expected AFFECTED_HEADS value, $2 = test name
    if [[ "$(cat "$HEADS_LOG")" != "$1" ]]; then
        echo "[FAIL] $2: expected heads.log '$1', got '$(cat "$HEADS_LOG")'" >&2
        exit 1
    fi
}

# 1. local feature -> remote main: full check
run_prepush "refs/heads/feature ${SHA_A} refs/heads/main ${SHA_B}
"
assert_rc 0 "feature->main"
assert_task_log "check" "feature->main"

# 2. local feature -> remote master: full check
run_prepush "refs/heads/feature ${SHA_A} refs/heads/master ${SHA_B}
"
assert_rc 0 "feature->master"
assert_task_log "check" "feature->master"

# 3. local feature -> other remote head: affected gates
run_prepush "refs/heads/feature ${SHA_A} refs/heads/some-feature ${SHA_B}
"
assert_rc 0 "feature->feature"
assert_task_log "affected" "feature->feature"
assert_heads_log "$SHA_A" "feature->feature"

# 3b. multiple feature heads union their exact pushed revisions
run_prepush "refs/heads/feature ${SHA_A} refs/heads/one ${SHA_B}
refs/heads/feature2 ${SHA_C} refs/heads/two ${SHA_B}
"
assert_rc 0 "multiple feature heads"
assert_task_log "affected" "multiple feature heads"
assert_heads_log "$SHA_A $SHA_C" "multiple feature heads"

# 4. tag push: no product gate
run_prepush "refs/heads/feature ${SHA_A} refs/tags/v1.0 ${SHA_B}
"
assert_rc 0 "tag push"
assert_task_log "" "tag push"

# 5. deletion to main: no product gate, even though remote is main
run_prepush "(delete) ${ZERO40} refs/heads/main ${SHA_B}
"
assert_rc 0 "deletion to main"
assert_task_log "" "deletion to main"

# 6. mixed: feature->feature (check-fast) and feature->main (check) => check, once
run_prepush "refs/heads/feature ${SHA_A} refs/heads/other ${SHA_B}
refs/heads/feature2 ${SHA_A} refs/heads/main ${SHA_B}
"
assert_rc 0 "mixed strongest wins"
assert_task_log "check" "mixed strongest wins"

# 7. mixed: deletion to main + push to feature => affected (deletion contributes nothing)
run_prepush "(delete) ${ZERO40} refs/heads/main ${SHA_B}
refs/heads/feature ${SHA_A} refs/heads/other ${SHA_B}
"
assert_rc 0 "mixed deletion + feature push"
assert_task_log "affected" "mixed deletion + feature push"

# 8. empty stdin: conservative affected fallback
run_prepush ""
assert_rc 0 "empty stdin"
assert_task_log "affected" "empty stdin"

# 9. blank stdin (only blank lines): conservative affected fallback
run_prepush "

"
assert_rc 0 "blank stdin"
assert_task_log "affected" "blank stdin"

# 10. malformed: wrong field count
run_prepush "refs/heads/feature ${SHA_A} refs/heads/main
"
assert_rc 64 "malformed field count"
assert_task_log "" "malformed field count"

# 11. malformed: non-hex sha
run_prepush "refs/heads/feature ${NONHEX40} refs/heads/main ${SHA_B}
"
assert_rc 64 "malformed non-hex sha"
assert_task_log "" "malformed non-hex sha"

# 12. malformed: wrong-length sha (SHA256-style repo out)
run_prepush "refs/heads/feature ${SHA256_A} refs/heads/main ${SHA_B}
"
assert_rc 64 "malformed sha length"
assert_task_log "" "malformed sha length"

# 13. malformed input still exits 64 even under SHATTER_FULL_PUSH=1
run_prepush "refs/heads/feature ${SHA256_A} refs/heads/main ${SHA_B}
" "SHATTER_FULL_PUSH=1"
assert_rc 64 "malformed sha length overrides FULL_PUSH"
assert_task_log "" "malformed sha length overrides FULL_PUSH"

# 14. SHATTER_FULL_PUSH=1 forces check even on a tag-only push
run_prepush "refs/heads/feature ${SHA_A} refs/tags/v1.0 ${SHA_B}
" "SHATTER_FULL_PUSH=1"
assert_rc 0 "tag push with SHATTER_FULL_PUSH"
assert_task_log "check" "tag push with SHATTER_FULL_PUSH"

echo "[ok] setup-hooks pre-push template classifies ref updates and picks the strongest gate"

### Pre-push input validation must run even when no gate can be invoked ###
# Regression: exit-64 malformed-input validation must not live inside the
# "Taskfile.yml exists and task is on PATH" guard — only the actual gate
# invocation may be skipped when those preconditions are missing.

run_prepush_env() {
    # $1 = stdin content, $2 = PATH to use for the hook invocation
    : > "$TASK_LOG"
    : > "$HEADS_LOG"
    set +e
    printf '%s' "$1" | env PATH="$2" TASK_LOG="$TASK_LOG" HEADS_LOG="$HEADS_LOG" \
        bash -c 'cd "$0" && exec "$1"' "$TEST_REPO" "$PRE_PUSH_HOOK" \
        > "$SCRATCH/prepush.out" 2>&1
    RC=$?
    set -e
}

# 15. malformed input still exits 64 when Taskfile.yml is absent
mv "$TEST_REPO/Taskfile.yml" "$SCRATCH/Taskfile.yml.bak"
run_prepush_env "refs/heads/feature ${SHA256_A} refs/heads/main ${SHA_B}
" "$TASK_BIN_DIR:$PATH"
assert_rc 64 "malformed sha length, Taskfile.yml absent"
assert_task_log "" "malformed sha length, Taskfile.yml absent"

# 16. valid input with Taskfile.yml absent: no crash, gate skipped, exit 0
run_prepush_env "refs/heads/feature ${SHA_A} refs/heads/main ${SHA_B}
" "$TASK_BIN_DIR:$PATH"
assert_rc 0 "valid feature->main push, Taskfile.yml absent"
assert_task_log "" "valid feature->main push, Taskfile.yml absent"
if ! grep -q "skipping check gate" "$SCRATCH/prepush.out"; then
    echo "[FAIL] expected skip message when Taskfile.yml absent" >&2
    cat "$SCRATCH/prepush.out" >&2
    exit 1
fi
mv "$SCRATCH/Taskfile.yml.bak" "$TEST_REPO/Taskfile.yml"

# 17. malformed input still exits 64 when `task` is missing from PATH
run_prepush_env "refs/heads/feature ${NONHEX40} refs/heads/main ${SHA_B}
" "/usr/bin:/bin"
assert_rc 64 "malformed non-hex sha, task missing from PATH"

# 18. valid input with `task` missing from PATH: no crash, gate skipped, exit 0
run_prepush_env "refs/heads/feature ${SHA_A} refs/heads/main ${SHA_B}
" "/usr/bin:/bin"
assert_rc 0 "valid feature->main push, task missing from PATH"
if ! grep -q "skipping check gate" "$SCRATCH/prepush.out"; then
    echo "[FAIL] expected skip message when task missing from PATH" >&2
    cat "$SCRATCH/prepush.out" >&2
    exit 1
fi

echo "[ok] setup-hooks pre-push malformed-input validation runs even without Taskfile.yml or task on PATH"
