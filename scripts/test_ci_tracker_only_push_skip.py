#!/usr/bin/env python3
"""Fixture/matrix test for str-35vtk.31: tracker-only pushes skip Actions.

Beads exports its full tracker snapshot to `.beads/issues.jsonl` on every
landing, which produces frequent `main` pushes whose *entire* diff is under
`.beads/**`. Those pushes don't need a full CI/release run. This test
replicates GitHub Actions' own `on.push.paths-ignore` semantics (a push is
skipped only when *every* changed path matches a paths-ignore pattern) and
runs it against the actual parsed workflow YAML, so a future edit that
removes or narrows the filter fails loudly.

Scope: only `ci.yml` and `release.yml` are push-triggered on `main` without
an existing path filter (checked via `bd show str-35vtk.31`). Other
push-triggered workflows either target tags (`docker-publish.yml`) or already
have a narrower `paths` filter (`devcontainer.yml`) and are out of scope.
Sync cadence and docs-only path filters are explicitly out of scope per the
issue.
"""

from __future__ import annotations

import re
from pathlib import Path
import unittest

import yaml

REPO_ROOT = Path(__file__).resolve().parents[1]
PUSH_TRIGGERED_WORKFLOWS = [
    REPO_ROOT / ".github" / "workflows" / "ci.yml",
    REPO_ROOT / ".github" / "workflows" / "release.yml",
]


def _glob_to_regex(pattern: str) -> re.Pattern:
    """Translate a GitHub Actions path-filter glob into an anchored regex.

    Supports the subset of glob syntax used by this repo's filters: literal
    path segments, `*` (matches within one path segment, not `/`), and `**`
    (matches zero or more path segments, including `/`). This is not a full
    reimplementation of GitHub's matcher, but it is exact for the patterns
    actually used here (e.g. `.beads/**`).
    """
    escaped = re.escape(pattern)
    # re.escape leaves '/' untouched and escapes '*' as '\*'. Swap out the
    # doubled-star sequence first so a lone '*' substitution below doesn't
    # also consume half of a '**'.
    escaped = escaped.replace(r"\*\*", "\x00DOUBLESTAR\x00")
    escaped = escaped.replace(r"\*", "[^/]*")
    escaped = escaped.replace("\x00DOUBLESTAR\x00", ".*")
    return re.compile(f"^{escaped}$")


def _matches_any(path: str, patterns: list[str]) -> bool:
    return any(_glob_to_regex(p).match(path) for p in patterns)


def push_would_trigger(changed_paths: list[str], paths_ignore: list[str] | None) -> bool:
    """Mirror GitHub's `on.push.paths-ignore` gating.

    A push-triggered job is skipped only when *every* changed path matches
    at least one `paths-ignore` pattern. If `paths_ignore` is empty/None,
    the push always triggers (today's behavior, and the behavior of any
    workflow with no filter at all).
    """
    if not paths_ignore:
        return True
    return not all(_matches_any(path, paths_ignore) for path in changed_paths)


def _load_push_paths_ignore(workflow_path: Path) -> list[str] | None:
    workflow = yaml.safe_load(workflow_path.read_text(encoding="utf-8"))
    # PyYAML parses the bare `on:` key as the boolean True in YAML 1.1.
    triggers = workflow.get("on") or workflow.get(True)
    push_cfg = triggers.get("push")
    if not isinstance(push_cfg, dict):
        return None
    return push_cfg.get("paths-ignore")


def _load_pull_request_cfg(workflow_path: Path) -> dict | None:
    workflow = yaml.safe_load(workflow_path.read_text(encoding="utf-8"))
    triggers = workflow.get("on") or workflow.get(True)
    pr_cfg = triggers.get("pull_request")
    return pr_cfg if isinstance(pr_cfg, dict) else None


class GlobMatcherUnitTests(unittest.TestCase):
    """Sanity-check the matcher itself against the pattern actually in use."""

    def test_beads_glob_matches_nested_paths(self):
        self.assertTrue(_matches_any(".beads/issues.jsonl", [".beads/**"]))
        self.assertTrue(_matches_any(".beads/sub/dir/file.txt", [".beads/**"]))

    def test_beads_glob_does_not_match_sibling_paths(self):
        self.assertFalse(_matches_any("shatter-core/src/lib.rs", [".beads/**"]))
        self.assertFalse(_matches_any(".github/workflows/ci.yml", [".beads/**"]))
        # Prefix collision: 'beads' without the leading dot must not match.
        self.assertFalse(_matches_any("beads/issues.jsonl", [".beads/**"]))


class TrackerOnlyPushMatrixTests(unittest.TestCase):
    """The tracker/mixed/workflow/source/PR matrix from str-35vtk.31."""

    TRACKER_ONLY = [".beads/issues.jsonl"]
    MIXED = [".beads/issues.jsonl", "shatter-core/src/lib.rs"]
    WORKFLOW_FILE_ONLY = [".github/workflows/ci.yml"]
    SOURCE_FILE_ONLY = ["shatter-core/src/lib.rs"]

    def test_tracker_only_push_is_skipped_on_every_push_triggered_workflow(self):
        for workflow_path in PUSH_TRIGGERED_WORKFLOWS:
            with self.subTest(workflow=workflow_path.name):
                paths_ignore = _load_push_paths_ignore(workflow_path)
                self.assertFalse(
                    push_would_trigger(self.TRACKER_ONLY, paths_ignore),
                    f"{workflow_path.name}: a push whose entire diff is under "
                    ".beads/** must be skipped, but the workflow's on.push "
                    f"paths-ignore ({paths_ignore!r}) would still trigger it",
                )

    def test_mixed_push_still_triggers(self):
        for workflow_path in PUSH_TRIGGERED_WORKFLOWS:
            with self.subTest(workflow=workflow_path.name):
                paths_ignore = _load_push_paths_ignore(workflow_path)
                self.assertTrue(
                    push_would_trigger(self.MIXED, paths_ignore),
                    f"{workflow_path.name}: a push that touches source "
                    "alongside .beads/** must still trigger normally",
                )

    def test_workflow_file_only_push_still_triggers(self):
        for workflow_path in PUSH_TRIGGERED_WORKFLOWS:
            with self.subTest(workflow=workflow_path.name):
                paths_ignore = _load_push_paths_ignore(workflow_path)
                self.assertTrue(
                    push_would_trigger(self.WORKFLOW_FILE_ONLY, paths_ignore),
                    f"{workflow_path.name}: a push that only touches "
                    "workflow files must still trigger normally",
                )

    def test_source_file_only_push_still_triggers(self):
        for workflow_path in PUSH_TRIGGERED_WORKFLOWS:
            with self.subTest(workflow=workflow_path.name):
                paths_ignore = _load_push_paths_ignore(workflow_path)
                self.assertTrue(
                    push_would_trigger(self.SOURCE_FILE_ONLY, paths_ignore),
                    f"{workflow_path.name}: an ordinary source-only push "
                    "must still trigger normally",
                )

    def test_pull_request_trigger_has_no_path_filter(self):
        """PR-triggered runs must be unaffected by the tracker-only skip.

        We assert the pull_request trigger carries no paths/paths-ignore key
        at all (its pre-existing, unfiltered shape), rather than simulating
        GitHub's PR-diff semantics, so this test fails loudly if a future
        edit narrows pull_request triggering as a side effect of this change.
        """
        for workflow_path in PUSH_TRIGGERED_WORKFLOWS:
            with self.subTest(workflow=workflow_path.name):
                pr_cfg = _load_pull_request_cfg(workflow_path)
                if pr_cfg is None:
                    # release.yml has no pull_request trigger; nothing to
                    # regress here for this workflow.
                    continue
                self.assertNotIn("paths", pr_cfg)
                self.assertNotIn("paths-ignore", pr_cfg)

    def test_no_manual_dispatch_workaround_introduced(self):
        """The issue explicitly rules out a workflow_dispatch workaround.

        ci.yml never had workflow_dispatch; this change must not add one as
        a side-channel to re-run CI on a skipped tracker-only push.
        """
        ci_workflow = yaml.safe_load(
            (REPO_ROOT / ".github" / "workflows" / "ci.yml").read_text(encoding="utf-8")
        )
        triggers = ci_workflow.get("on") or ci_workflow.get(True)
        self.assertNotIn("workflow_dispatch", triggers)


if __name__ == "__main__":
    unittest.main()
