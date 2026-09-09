#!/usr/bin/env bash
# Shared helper for scripts/test_*.sh fixtures that build throwaway git repos.
#
# git hooks (pre-commit, pre-push, post-checkout, ...) export GIT_DIR (and
# usually GIT_WORK_TREE) in the environment, pointing at the real invoking
# repository, before running any hook script. `git -C <tempdir>` and
# `cd <tempdir>` do NOT override an explicit GIT_DIR env var -- repository
# discovery consults the env var first -- so any git-sandbox test that
# builds its own throwaway fixture repo will silently operate on the real
# repo instead when the test happens to run from inside a hook (e.g.
# `task check` invoked from `pre-push`). See str-jttrf.
#
# Source this file and it isolates the current shell's git-discovery env
# immediately (before any git operation runs), so every git-sandbox test
# only needs one line:
#
#   source "$REPO_ROOT/scripts/git-sandbox-test-lib.sh"
#
# placed before the fixture's first `git init`/`git clone`/`git -C` call.

git_sandbox_isolate_env() {
    unset GIT_DIR GIT_WORK_TREE GIT_INDEX_FILE GIT_OBJECT_DIRECTORY \
        GIT_ALTERNATE_OBJECT_DIRECTORIES GIT_COMMON_DIR
}

git_sandbox_isolate_env
