#!/usr/bin/env python3
"""Structural regression test for the CI workflow (str-35vtk.21).

CI must invoke the full landing gate (`task check`) exactly once and must
not run parity/conformance as standalone steps outside of it, since
`task check` (check-static -> check-unit -> check-integration) already
covers test-standard, parity, and conformance. This test parses the actual
workflow YAML so a future edit that reintroduces a duplicate gate or a
standalone parity/conformance step fails loudly instead of silently
regressing CI runtime.
"""

from __future__ import annotations

import re
from pathlib import Path
import unittest

import yaml


REPO_ROOT = Path(__file__).resolve().parents[1]
CI_WORKFLOW = REPO_ROOT / ".github" / "workflows" / "ci.yml"

# Matches a `run:` step whose command is exactly `task <name>` (optionally
# with leading/trailing whitespace or blank lines), not a substring match
# like `task check-static` or a step that happens to mention `task check`
# in passing (e.g. a comment).
TASK_COMMAND_RE = re.compile(r"^\s*task\s+(?P<name>[A-Za-z0-9:_-]+)\s*$")

REQUIRED_SETUP_ACTIONS = [
    "actions/checkout",
    "dtolnay/rust-toolchain",
    "actions/setup-node",
    "actions/setup-go",
    "arduino/setup-task",
    "actions/cache",
]

STANDALONE_GATES_FOLDED_INTO_CHECK = {"parity", "conformance", "test-standard"}

# shatter-llm is a real Cargo workspace member, but `task check`'s
# check-static/check-unit deps clippy/test individual crates (core:clippy,
# cli:clippy, ...) rather than the workspace-wide `workspace-clippy` /
# `workspace-test` tasks the removed `task test-standard` step used, so it
# is not covered by `task check`. Until that's folded into Taskfile.yml,
# ci.yml must run these explicitly.
REQUIRED_SHATTER_LLM_COMMANDS = [
    re.compile(r"^\s*cargo\s+clippy\s+-p\s+shatter-llm\b"),
    re.compile(r"^\s*cargo\s+test\s+-p\s+shatter-llm\b"),
]


def _iter_run_step_task_commands(workflow: dict):
    """Yield the bare `task <name>` command from every run step in every job."""
    for job in workflow.get("jobs", {}).values():
        for step in job.get("steps", []):
            run = step.get("run")
            if not isinstance(run, str):
                continue
            for line in run.splitlines():
                match = TASK_COMMAND_RE.match(line)
                if match:
                    yield match.group("name")


def _iter_run_lines(workflow: dict):
    for job in workflow.get("jobs", {}).values():
        for step in job.get("steps", []):
            run = step.get("run")
            if not isinstance(run, str):
                continue
            yield from run.splitlines()


class CiWorkflowStructureTests(unittest.TestCase):
    def setUp(self):
        self.raw = CI_WORKFLOW.read_text(encoding="utf-8")
        self.workflow = yaml.safe_load(self.raw)

    def test_task_check_invoked_exactly_once(self):
        task_commands = list(_iter_run_step_task_commands(self.workflow))
        check_invocations = [name for name in task_commands if name == "check"]
        self.assertEqual(
            check_invocations,
            ["check"],
            f"expected exactly one bare `task check` step, found commands: {task_commands}",
        )

    def test_no_standalone_folded_in_gates(self):
        task_commands = set(_iter_run_step_task_commands(self.workflow))
        offenders = task_commands & STANDALONE_GATES_FOLDED_INTO_CHECK
        self.assertFalse(
            offenders,
            f"found standalone step(s) for gate(s) already covered by `task check`: {sorted(offenders)}",
        )

    def test_shatter_llm_clippy_and_test_present(self):
        run_lines = list(_iter_run_lines(self.workflow))
        for pattern in REQUIRED_SHATTER_LLM_COMMANDS:
            self.assertTrue(
                any(pattern.match(line) for line in run_lines),
                f"expected a run step matching {pattern.pattern!r} covering the "
                "shatter-llm workspace member, which `task check` does not clippy/test "
                "(see the str-35vtk.21 bd note)",
            )

    def test_setup_and_cache_steps_retained(self):
        uses_values = [
            step.get("uses", "")
            for job in self.workflow.get("jobs", {}).values()
            for step in job.get("steps", [])
        ]
        for action in REQUIRED_SETUP_ACTIONS:
            self.assertTrue(
                any(uses.startswith(action) for uses in uses_values),
                f"expected a step using `{action}` to remain in {CI_WORKFLOW.relative_to(REPO_ROOT)}",
            )


if __name__ == "__main__":
    unittest.main()
