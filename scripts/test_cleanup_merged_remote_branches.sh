#!/usr/bin/env bash
# Regression test for scripts/cleanup-merged-remote-branches.sh (str-g18x).
#
# `git for-each-ref --format='%(refname:short)' refs/remotes/<remote>` renders
# the remote's symbolic default-branch pointer (`refs/remotes/<remote>/HEAD`)
# as just the bare remote name (e.g. "origin"), not "<remote>/HEAD". Before
# this fix, that name slipped past the script's ""|main|HEAD exclusion list
# and into the deletable-branch set, so `--execute` would run
# `git push <remote> --delete <remote>` — attempting to delete a real branch
# sharing the remote's name if one exists, instead of skipping the symref as
# intended.
#
# Builds a throwaway bare "remote" + working clone (never touches the real
# project's git history or its actual `origin`), pushes one branch fully
# merged into main and one branch that is NOT merged, and asserts a dry run:
#   1. never lists the bare remote name itself ("origin") as deletable;
#   2. lists the merged branch as deletable;
#   3. does not list the unmerged branch at all (for-each-ref --merged
#      already excludes it, so it must not appear as deletable OR protected).
#
# Usage: bash scripts/test_cleanup_merged_remote_branches.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCRIPT_UNDER_TEST="$REPO_ROOT/scripts/cleanup-merged-remote-branches.sh"

WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

BARE="$WORKDIR/bare-remote.git"
CLONE="$WORKDIR/clone"

git init --quiet --bare "$BARE"
# Set the bare "remote"'s HEAD symref explicitly rather than relying on the
# invoking environment's `init.defaultBranch`: this dev environment has it
# set to "main" globally, but CI runners may not, in which case the bare
# repo's HEAD would default to "master" (or be unset) while only "main" is
# ever pushed below, making `git remote set-head origin -a` fail with
# "Cannot determine remote HEAD" and abort this whole test under set -e.
git -C "$BARE" symbolic-ref HEAD refs/heads/main

git init --quiet -b main "$CLONE"
cd "$CLONE"
git config user.email "test@example.com"
git config user.name "Test"
git remote add origin "$BARE"

echo "root" > file.txt
git add file.txt
git commit --quiet -m "root commit"
git push --quiet -u origin main

# `refs/remotes/<remote>/HEAD` is normally created by `git clone` (which
# implicitly runs `git remote set-head origin -a`), not by a plain
# `git remote add` + `git push -u`. Create it explicitly so this test
# actually exercises the bug this script guards against — without this,
# the symref never exists and the regression can't reproduce.
git remote set-head origin -a > /dev/null

# A branch fully merged into main — should be deletable.
git checkout --quiet -b merged-branch
echo "merged" >> file.txt
git commit --quiet -am "merged branch commit"
git checkout --quiet main
git merge --quiet --no-ff merged-branch -m "merge merged-branch"
git push --quiet origin main merged-branch

# A branch NOT merged into main — must never appear as deletable.
git checkout --quiet -b unmerged-branch
echo "unmerged" >> file.txt
git commit --quiet -am "unmerged branch commit"
git push --quiet origin unmerged-branch
git checkout --quiet main

# Copy the script under test into this throwaway repo: it resolves its own
# project root relative to `$0`'s location (`cd "$(dirname "$0")/.."`), so it
# must live under a `scripts/` dir inside the fake repo to operate on it
# rather than the real project.
mkdir -p scripts
cp "$SCRIPT_UNDER_TEST" scripts/cleanup-merged-remote-branches.sh

OUTPUT="$(bash scripts/cleanup-merged-remote-branches.sh --remote origin --skip-bd 2>&1)"

fail() {
    echo "[FAIL] $1" >&2
    echo "       full output:" >&2
    echo "$OUTPUT" >&2
    exit 1
}

if echo "$OUTPUT" | grep -qE '^\s*del\s+origin\s*$'; then
    fail "the bare remote name ('origin', the HEAD symref) must never be listed as a deletable branch"
fi

if ! echo "$OUTPUT" | grep -qE '^\s*del\s+merged-branch\s*$'; then
    fail "a branch fully merged into main must be listed as deletable"
fi

if echo "$OUTPUT" | grep -qE '\bunmerged-branch\b'; then
    fail "a branch NOT merged into main must not appear as deletable or protected at all"
fi

echo "[ok] cleanup-merged-remote-branches: origin/HEAD symref excluded, merged branch listed, unmerged branch absent"

# ── Regression test for str-vsgny: the final per-branch live-bd re-check ────
# Verifies the fourth (cross-machine) protection layer added in str-vsgny: when
# --execute runs, a branch matching an issue id that `bd show` reports as
# in_progress must be skipped even though it was already classified as a
# deletable candidate (simulates a claim from another machine that arrived
# after classification but before this branch's deletion turn). Also checks
# that a branch whose `bd show` succeeds and reports the issue closed/
# unassigned is still deleted (the new layer must not become a blanket veto).
#
# Uses a fake `bd` shim on PATH (never touches the real project's bd/Dolt
# state) and a fake `git` shim that intercepts only `push --delete` (records
# the attempt instead of touching a real remote) while passing every other
# git invocation through to the real git.

WORKDIR2="$(mktemp -d)"
trap 'rm -rf "$WORKDIR" "$WORKDIR2"' EXIT

BARE2="$WORKDIR2/bare-remote.git"
CLONE2="$WORKDIR2/clone"
FAKEBIN="$WORKDIR2/fakebin"
mkdir -p "$FAKEBIN"

git init --quiet --bare "$BARE2"
# See the matching comment on $BARE above: don't rely on the invoking
# environment's `init.defaultBranch` for this bare repo's HEAD either.
git -C "$BARE2" symbolic-ref HEAD refs/heads/main
git init --quiet -b main "$CLONE2"
cd "$CLONE2"
git config user.email "test@example.com"
git config user.name "Test"
git remote add origin "$BARE2"

echo "root" > file.txt
git add file.txt
git commit --quiet -m "root commit"
git push --quiet -u origin main
git remote set-head origin -a > /dev/null

# Two branches, both fully merged into main and both otherwise deletable
# (no local worktree, not in the JSONL protected set — there is no
# .beads/issues.jsonl in this throwaway repo at all).
for name in str-fake1-claimed-elsewhere str-fake2-safe; do
    git checkout --quiet -b "$name"
    echo "$name" >> file.txt
    git commit --quiet -am "commit on $name"
    git checkout --quiet main
    git merge --quiet --no-ff "$name" -m "merge $name"
done
git push --quiet origin main str-fake1-claimed-elsewhere str-fake2-safe

mkdir -p scripts
cp "$SCRIPT_UNDER_TEST" scripts/cleanup-merged-remote-branches.sh

# Fake `bd`: reports str-fake1 in_progress (protected), str-fake2 closed/
# unassigned (safe), anything else not found (bd show would exit non-zero).
cat > "$FAKEBIN/bd" <<'FAKEBD'
#!/usr/bin/env bash
if [ "$1" = "list" ]; then
    echo "[]"
    exit 0
fi
if [ "$1" = "show" ]; then
    # cleanup-merged-remote-branches.sh's issue_id() extracts only the
    # str-<alnum> prefix (stops at the first hyphen after it), so it queries
    # `bd show str-fake1` / `bd show str-fake2`, not the full branch name.
    case "$2" in
        str-fake1)
            echo '[{"id":"str-fake1","status":"in_progress","assignee":"someone-else"}]'
            ;;
        str-fake2)
            echo '[{"id":"str-fake2","status":"closed","assignee":null}]'
            ;;
        *)
            exit 1
            ;;
    esac
    exit 0
fi
exit 1
FAKEBD
chmod +x "$FAKEBIN/bd"

# Fake `git`: intercept only `push <remote> --delete <branch>` to avoid any
# real network/remote deletion; delegate everything else to the real git.
REAL_GIT="$(command -v git)"
cat > "$FAKEBIN/git" <<FAKEGIT
#!/usr/bin/env bash
if [ "\$1" = "push" ]; then
    for a in "\$@"; do
        if [ "\$a" = "--delete" ]; then
            echo "FAKE-DELETE: \$*" >> "$WORKDIR2/deletes.log"
            exit 0
        fi
    done
fi
exec "$REAL_GIT" "\$@"
FAKEGIT
chmod +x "$FAKEBIN/git"

EXEC_OUTPUT="$(PATH="$FAKEBIN:$PATH" bash scripts/cleanup-merged-remote-branches.sh --remote origin --execute 2>&1)"

fail2() {
    echo "[FAIL] $1" >&2
    echo "       full output:" >&2
    echo "$EXEC_OUTPUT" >&2
    exit 1
}

if ! echo "$EXEC_OUTPUT" | grep -qE 'SKIP.*str-fake1-claimed-elsewhere.*live bd re-check'; then
    fail2 "a branch whose live bd re-check reports in_progress must be SKIPped by the fourth protection layer, not deleted"
fi

if [ -f "$WORKDIR2/deletes.log" ] && grep -q "str-fake1-claimed-elsewhere" "$WORKDIR2/deletes.log"; then
    fail2 "str-fake1-claimed-elsewhere must never reach git push --delete"
fi

if ! echo "$EXEC_OUTPUT" | grep -qE 'deleted str-fake2-safe'; then
    fail2 "a branch whose live bd re-check reports closed/unassigned must still be deleted (the new layer must not become a blanket veto)"
fi

echo "[ok] cleanup-merged-remote-branches: fourth protection layer (live per-branch bd re-check) blocks an in-progress/claimed branch at --execute time while still deleting a confirmed-safe one"
