"""Shared helper for scripts/test_*.py fixtures that build throwaway git repos.

git hooks (pre-commit, pre-push, post-checkout, ...) export `GIT_DIR` (and
usually `GIT_WORK_TREE`) in the environment, pointing at the real invoking
repository, before running any hook script. Passing `cwd=` to
`subprocess.run` does NOT override an explicit `GIT_DIR` env var --
repository discovery consults the env var first -- so any test that builds
its own throwaway fixture repo and runs `git` against it via `cwd=` will
silently operate on the real repo instead when the test happens to run from
inside a hook (e.g. `task check` invoked from `pre-push`). See str-jttrf.

Call `sanitized_git_env()` and pass the result as `subprocess.run(...,
env=...)` for every git invocation that targets a fixture repo, starting
with the very first one (typically `git init`).
"""

from __future__ import annotations

import os

# The comprehensive set of env vars git consults for local repository
# discovery, mirroring scripts/examples_checkout.py's GIT_LOCAL_ENV_VARS.
GIT_SANDBOX_ENV_VARS = (
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_CONFIG",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_PARAMETERS",
    "GIT_DIR",
    "GIT_GRAFT_FILE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_OBJECT_DIRECTORY",
    "GIT_PREFIX",
    "GIT_REPLACE_REF_BASE",
    "GIT_SHALLOW_FILE",
    "GIT_WORK_TREE",
)


def sanitized_git_env(base: dict[str, str] | None = None) -> dict[str, str]:
    """Return a copy of `base` (default `os.environ`) with git's
    local-repository-discovery env vars stripped, so a `git` subprocess run
    against a throwaway fixture directory cannot see -- or mutate -- the
    real invoking repository even when `GIT_DIR` is preset in the
    environment."""
    env = dict(base if base is not None else os.environ)
    for key in GIT_SANDBOX_ENV_VARS:
        env.pop(key, None)
    return env
