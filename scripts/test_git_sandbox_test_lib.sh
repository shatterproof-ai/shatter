#!/usr/bin/env bash
# Regression test for scripts/git-sandbox-test-lib.sh (str-jttrf).
#
# git hooks export GIT_DIR/GIT_WORK_TREE pointing at the real invoking repo
# before running any hook script. `-C <tempdir>` does not override an
# explicit GIT_DIR, so a git-sandbox test that forgets to isolate its
# environment silently targets the real repo instead of its throwaway
# fixture. This proves that sourcing git-sandbox-test-lib.sh neutralizes a
# pre-set GIT_DIR/GIT_WORK_TREE (and friends) so a subsequently built
# fixture repo cannot see or mutate a real repo the leaked env points at.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LIB="$REPO_ROOT/scripts/git-sandbox-test-lib.sh"
# Isolate bootstrap too: the enclosing hook may already point at a real repo.
# shellcheck source=scripts/git-sandbox-test-lib.sh
source "$LIB"

SCRATCH="$(mktemp -d)"
trap 'rm -rf "$SCRATCH"' EXIT

fail() {
    echo "[FAIL] $*" >&2
    exit 1
}

# ── Build a "real" repo that the leaked env will point at ──────────────────
REAL_REPO="$SCRATCH/real-repo"
git init -q "$REAL_REPO"
git -C "$REAL_REPO" config user.email "test@example.com"
git -C "$REAL_REPO" config user.name "Test"
echo real >"$REAL_REPO/README"
git -C "$REAL_REPO" add README
git -C "$REAL_REPO" commit -q -m "real repo init"
REAL_HEAD_BEFORE="$(git -C "$REAL_REPO" rev-parse HEAD)"

# ── Simulate a hook environment: GIT_DIR/GIT_WORK_TREE leaked in ───────────
export GIT_DIR="$REAL_REPO/.git"
export GIT_WORK_TREE="$REAL_REPO"
export GIT_INDEX_FILE="$REAL_REPO/.git/index"
export GIT_OBJECT_DIRECTORY="$REAL_REPO/.git/objects"
export GIT_ALTERNATE_OBJECT_DIRECTORIES="$REAL_REPO/.git/objects"
export GIT_COMMON_DIR="$REAL_REPO/.git"

# Sanity check: without isolation, a plain `git` invocation in an unrelated
# cwd really does resolve to the leaked real repo (proves the simulated
# hook environment reproduces the bug this test guards against).
UNISOLATED_TOPLEVEL="$(cd "$SCRATCH" && git rev-parse --show-toplevel)"
[[ "$UNISOLATED_TOPLEVEL" == "$(cd "$REAL_REPO" && pwd)" ]] \
    || fail "test setup did not reproduce the GIT_DIR leak (got toplevel: $UNISOLATED_TOPLEVEL)"

# ── Source the helper: it must isolate the current shell's git env ─────────
# shellcheck source=scripts/git-sandbox-test-lib.sh
source "$LIB"

for var in GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_OBJECT_DIRECTORY \
    GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_COMMON_DIR; do
    if [[ -n "${!var:-}" ]]; then
        fail "sourcing git-sandbox-test-lib.sh must unset $var, still set to '${!var}'"
    fi
done

# ── Build a throwaway fixture repo exactly as a test script would ──────────
FIXTURE="$SCRATCH/fixture"
git init -q "$FIXTURE"
git -C "$FIXTURE" config user.email "test@example.com"
git -C "$FIXTURE" config user.name "Test"
git -C "$FIXTURE" checkout -q -b sandbox-marker-branch
echo fixture >"$FIXTURE/marker.txt"
git -C "$FIXTURE" add marker.txt
git -C "$FIXTURE" commit -q -m "fixture commit"

# ── The fixture op must have landed in FIXTURE, not REAL_REPO ──────────────
FIXTURE_TOPLEVEL="$(git -C "$FIXTURE" rev-parse --show-toplevel)"
[[ "$FIXTURE_TOPLEVEL" == "$(cd "$FIXTURE" && pwd)" ]] \
    || fail "fixture git op resolved to '$FIXTURE_TOPLEVEL', not the fixture directory"

if git -C "$REAL_REPO" show-ref --quiet refs/heads/sandbox-marker-branch; then
    fail "fixture branch leaked into the real repo despite isolation"
fi

REAL_HEAD_AFTER="$(git -C "$REAL_REPO" rev-parse HEAD)"
[[ "$REAL_HEAD_AFTER" == "$REAL_HEAD_BEFORE" ]] \
    || fail "real repo HEAD moved from $REAL_HEAD_BEFORE to $REAL_HEAD_AFTER; fixture ops must never mutate it"

REAL_BRANCH_COUNT="$(git -C "$REAL_REPO" branch --list | wc -l | tr -d ' ')"
[[ "$REAL_BRANCH_COUNT" == "1" ]] \
    || fail "real repo must still have exactly one branch, has $REAL_BRANCH_COUNT"

echo "[ok] git-sandbox-test-lib.sh isolates a leaked GIT_DIR/GIT_WORK_TREE so fixture git ops cannot see or mutate the real repo"
