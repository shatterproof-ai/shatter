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
