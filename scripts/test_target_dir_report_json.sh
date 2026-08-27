#!/usr/bin/env bash
# Golden tests for `scripts/target-dir-report.sh --json` (str-35vtk.28).
#
# Builds a throwaway git repo with real linked worktrees so branch/primary
# detection reflects actual `git worktree list --porcelain` output, then
# checks the v1 JSON schema against exact expected values for the absent,
# hardlink, symlink, unknown-issue, current, and primary cases plus the
# overall total_bytes.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT="$REPO_ROOT/scripts/target-dir-report.sh"
SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

fail() {
    echo "[FAIL] $*" >&2
    exit 1
}

# ── Build a throwaway repo with three linked worktrees ──────────────────────
BASE_REPO="$SCRATCH/base"
git init -q "$BASE_REPO"
git -C "$BASE_REPO" config user.email "test@example.com"
git -C "$BASE_REPO" config user.name "Test"
echo hello >"$BASE_REPO/README"
git -C "$BASE_REPO" add README
git -C "$BASE_REPO" commit -q -m init

WT_MATCH="$SCRATCH/wt-match"          # branch str-abc1.2: known issue, hardlink + symlink + absent target
WT_UNKNOWN_ID="$SCRATCH/wt-unknown-id" # branch str-zzzz9: matches issue-id pattern, absent from tracker
WT_NO_MATCH="$SCRATCH/wt-no-match"     # branch misc-branch: no issue-id pattern match at all

git -C "$BASE_REPO" worktree add -q -b str-abc1.2 "$WT_MATCH" >/dev/null
git -C "$BASE_REPO" worktree add -q -b str-zzzz9 "$WT_UNKNOWN_ID" >/dev/null
git -C "$BASE_REPO" worktree add -q -b misc-branch "$WT_NO_MATCH" >/dev/null

BEADS_JSONL="$SCRATCH/issues.jsonl"
cat >"$BEADS_JSONL" <<'EOF'
{"_type":"issue","id":"str-abc1.2","status":"in_progress"}
EOF

# WT_MATCH/target: one file + one hardlink to it (counted twice, once per
# directory entry) + one symlink (excluded entirely). shatter-rust/target and
# the runtime target are left absent.
mkdir -p "$WT_MATCH/target"
dd if=/dev/zero of="$WT_MATCH/target/a.bin" bs=1024 count=4 status=none
ln "$WT_MATCH/target/a.bin" "$WT_MATCH/target/a-hardlink.bin"
ln -s a.bin "$WT_MATCH/target/a-symlink.bin"

# WT_UNKNOWN_ID/shatter-rust/target: exists but empty (bytes 0, exists true).
mkdir -p "$WT_UNKNOWN_ID/shatter-rust/target"

# WT_NO_MATCH/shatter-rust-runtime/target: one plain file.
mkdir -p "$WT_NO_MATCH/shatter-rust-runtime/target"
dd if=/dev/zero of="$WT_NO_MATCH/shatter-rust-runtime/target/r.bin" bs=1024 count=2 status=none

EXPECTED_MATCH_ROOT_BYTES=$((4 * 1024 * 2))
EXPECTED_NO_MATCH_RUNTIME_BYTES=$((2 * 1024))
EXPECTED_TOTAL=$((EXPECTED_MATCH_ROOT_BYTES + EXPECTED_NO_MATCH_RUNTIME_BYTES))

# Run from inside WT_MATCH (no --worktree) so git auto-discovers all four
# worktrees and "current" exercises a real invoking-PWD match.
OUTPUT="$(cd "$WT_MATCH" && TARGET_DIR_REPORT_BEADS_JSONL="$BEADS_JSONL" "$SCRIPT" --json)"

jq -e . >/dev/null <<<"$OUTPUT" || fail "output is not valid JSON"

wt() { # wt <path> <jq-filter>
    jq -e --arg p "$1" ".worktrees[] | select(.path == \$p) | $2" <<<"$OUTPUT"
}

# ── absent ────────────────────────────────────────────────────────────────
[[ "$(wt "$WT_MATCH" '.targets[] | select(.kind == "shatter-rust target") | .exists')" == "false" ]] \
    || fail "absent shatter-rust target must report exists:false"
[[ "$(wt "$WT_MATCH" '.targets[] | select(.kind == "shatter-rust target") | .bytes')" == "0" ]] \
    || fail "absent target must report bytes:0"

# ── hardlink + symlink (exact byte count) ───────────────────────────────────
ROOT_BYTES="$(wt "$WT_MATCH" '.targets[] | select(.kind == "root target") | .bytes')"
[[ "$ROOT_BYTES" == "$EXPECTED_MATCH_ROOT_BYTES" ]] \
    || fail "root target bytes: expected $EXPECTED_MATCH_ROOT_BYTES (hardlink counted twice, symlink excluded), got $ROOT_BYTES"

# ── unknown: pattern matches but issue absent from tracker ─────────────────
[[ "$(wt "$WT_UNKNOWN_ID" '.issue_id')" == '"str-zzzz9"' ]] \
    || fail "str-zzzz9 branch must yield issue_id str-zzzz9"
[[ "$(wt "$WT_UNKNOWN_ID" '.issue_status')" == '"unknown"' ]] \
    || fail "issue absent from tracker must yield issue_status unknown"

# ── unknown: branch has no issue-id pattern at all ──────────────────────────
[[ "$(wt "$WT_NO_MATCH" '.issue_id')" == "null" ]] \
    || fail "misc-branch must yield issue_id null"
[[ "$(wt "$WT_NO_MATCH" '.issue_status')" == '"unknown"' ]] \
    || fail "misc-branch must yield issue_status unknown"

# ── known issue resolves a real status ──────────────────────────────────────
[[ "$(wt "$WT_MATCH" '.issue_id')" == '"str-abc1.2"' ]] \
    || fail "str-abc1.2 branch must yield issue_id str-abc1.2"
[[ "$(wt "$WT_MATCH" '.issue_status')" == '"in_progress"' ]] \
    || fail "known issue must resolve its tracked status"

# ── current / primary ───────────────────────────────────────────────────────
[[ "$(wt "$WT_MATCH" '.current')" == "true" ]] || fail "invoking worktree must report current:true"
[[ "$(wt "$WT_UNKNOWN_ID" '.current')" == "false" ]] || fail "non-invoking worktree must report current:false"
[[ "$(wt "$BASE_REPO" '.primary')" == "true" ]] || fail "base checkout must report primary:true"
[[ "$(wt "$WT_MATCH" '.primary')" == "false" ]] || fail "linked worktree must report primary:false"

# ── sort order: worktrees by path, targets by kind ──────────────────────────
SORTED_PATHS="$(jq -r '[.worktrees[].path] as $p | ($p == ($p | sort)) | tostring' <<<"$OUTPUT")"
[[ "$SORTED_PATHS" == "true" ]] || fail "worktrees must be sorted by path"
KIND_ORDER="$(wt "$WT_MATCH" '[.targets[].kind]' | jq -c .)"
[[ "$KIND_ORDER" == '["root target","runtime target","shatter-rust target"]' ]] \
    || fail "targets must be sorted by kind, got: $KIND_ORDER"

# ── exact total ──────────────────────────────────────────────────────────────
TOTAL="$(jq -e '.total_bytes' <<<"$OUTPUT")"
[[ "$TOTAL" == "$EXPECTED_TOTAL" ]] \
    || fail "total_bytes: expected $EXPECTED_TOTAL, got $TOTAL"

# ── --json is orthogonal: human TSV output is untouched by its presence ────
HUMAN_OUTPUT="$(cd "$WT_MATCH" && "$SCRIPT")"
HUMAN_HEADER="$(head -n1 <<<"$HUMAN_OUTPUT")"
[[ "$HUMAN_HEADER" == $'KIB\tSIZE\tTARGET_DIR' ]] \
    || fail "human-mode header must be unchanged when --json is not passed"

# ── regression: a permission-denied subdirectory must degrade, not crash ───
# find exits non-zero on an unreadable subtree; under `set -euo pipefail`
# that must not abort the whole --json invocation (it must report a
# best-effort partial size for the affected target instead).
PERM_ROOT="$SCRATCH/perm-wt"
PERM_STDERR_FILE="$SCRATCH/perm-wt-stderr"
mkdir -p "$PERM_ROOT/target/readable" "$PERM_ROOT/target/locked"
dd if=/dev/zero of="$PERM_ROOT/target/readable/a.bin" bs=1024 count=2 status=none
dd if=/dev/zero of="$PERM_ROOT/target/locked/b.bin" bs=1024 count=4 status=none
chmod 000 "$PERM_ROOT/target/locked"
trap 'chmod 755 "$PERM_ROOT/target/locked" 2>/dev/null; rm -rf "$SCRATCH"' EXIT

if PERM_OUTPUT="$("$SCRIPT" --json --worktree "$PERM_ROOT" 2>"$PERM_STDERR_FILE")"; then
    PERM_EXIT=0
else
    PERM_EXIT=$?
fi
PERM_STDERR="$(cat "$PERM_STDERR_FILE")"
chmod 755 "$PERM_ROOT/target/locked"

[[ "$PERM_EXIT" -eq 0 ]] || fail "an unreadable subdirectory must not abort the --json invocation"
jq -e . >/dev/null <<<"$PERM_OUTPUT" || fail "output with an unreadable subdirectory must still be valid JSON"
[[ "$(jq -r '.worktrees[0].targets[] | select(.kind == "root target") | .bytes' <<<"$PERM_OUTPUT")" == "2048" ]] \
    || fail "an unreadable subdirectory must not drop the readable sibling's bytes from the partial total"
[[ "$PERM_STDERR" == *"warning:"* ]] \
    || fail "an unreadable subdirectory must produce a warning, not silent data loss"

# ── regression: a target's path must be resolved, not the raw --worktree arg ─
# The worktree-level `path` is a realpath; every target's `path` must be
# derived from that same resolved base, not the literal (possibly relative
# or symlinked) --worktree argument. Run from inside BASE_REPO (a real git
# repo) rather than SCRATCH so the script's own `git worktree list` call
# succeeds cleanly instead of just degrading on a "not a git repository" error.
REL_ROOT="$BASE_REPO/rel-wt"
mkdir -p "$REL_ROOT/target"
dd if=/dev/zero of="$REL_ROOT/target/a.bin" bs=1024 count=1 status=none
REL_OUTPUT="$(cd "$BASE_REPO" && "$SCRIPT" --json --worktree "rel-wt")"
WT_PATH="$(jq -r '.worktrees[0].path' <<<"$REL_OUTPUT")"
TARGET_PATH="$(jq -r '.worktrees[0].targets[] | select(.kind == "root target") | .path' <<<"$REL_OUTPUT")"
[[ "$WT_PATH" == /* ]] || fail "worktree path must be resolved to an absolute path"
[[ "$TARGET_PATH" == "$WT_PATH/target" ]] \
    || fail "target path must be derived from the resolved worktree path, got worktree=$WT_PATH target=$TARGET_PATH"

echo "[ok] target-dir-report --json produces the v1 schema with exact byte counts"
