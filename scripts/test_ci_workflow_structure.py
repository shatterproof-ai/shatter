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

import glob
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


# --- Path contract for every workflow (str-49drv.24) ------------------------
#
# A workflow that points setup-go at a file that does not exist fails ~20s
# into every run (drift-patrol.yml did, 7/7 scheduled runs).
#
# COVERED path kinds (each must exist in the checkout; globs must match):
#   - with.go-version-file, with.cache-dependency-path (one path per line),
#     with.node-version-file
#   - step-level and job-default working-directory
#   - single-quoted arguments of hashFiles(...) in `with:` and job `env:` values
#
# NOT covered (a wrong path of these kinds still goes undetected):
#   - `uses: ./local-action` references
#   - with.path and other `with:` inputs not listed above
#   - on.*.paths / paths-ignore trigger filters
#   - repo-relative scripts or files named inside `run:` commands
#   - path values containing `${{ ... }}` other than a plain hashFiles call
#     (reported as skipped, see EXPECTED_SKIPS)
#   - hashFiles(...) calls whose arguments are not all single-quoted literals
#     (reported as skipped, see EXPECTED_SKIPS)

WORKFLOWS_DIR = REPO_ROOT / ".github" / "workflows"

# Paths a workflow creates at run time, exempt from the existence check.
# Each entry needs a one-line reason. Empty today: every working-directory in
# the workflows is a checked-in directory.
RUNTIME_PATH_ALLOWLIST: dict[str, str] = {}

# Values the checker skipped on purpose, as "workflow.yml: location: 'value'
# (reason)" strings. The test fails on any skip not listed here, so a skip is
# never silent. Empty today: no workflow needs one.
EXPECTED_SKIPS: set[str] = set()

PATH_INPUT_KEYS = ("go-version-file", "cache-dependency-path", "node-version-file")
GLOB_CHARS = ("*", "?", "[")
HASHFILES_RE = re.compile(r"hashFiles\((?P<args>[^)]*)\)")
QUOTED_ARG_RE = re.compile(r"'([^']*)'")


def _workflow_path_refs(workflow: dict):
    """Yield (location, value) for every path-valued field in a workflow."""
    for job_name, job in (workflow.get("jobs") or {}).items():
        wd = (job.get("defaults") or {}).get("run", {}).get("working-directory")
        if wd is not None:
            yield f"jobs.{job_name}.defaults.run.working-directory", str(wd)
        for index, step in enumerate(job.get("steps") or []):
            where = f"jobs.{job_name}.steps[{index}]"
            if "working-directory" in step:
                yield f"{where}.working-directory", str(step["working-directory"])
            with_ = step.get("with") or {}
            for key in PATH_INPUT_KEYS:
                if key in with_:
                    yield f"{where}.with.{key}", str(with_[key])
            for key, value in with_.items():
                if isinstance(value, str) and "hashFiles(" in value:
                    yield f"{where}.with.{key}", value
        for key, value in (job.get("env") or {}).items():
            if isinstance(value, str) and "hashFiles(" in value:
                yield f"jobs.{job_name}.env.{key}", value


def _hashfiles_globs(value: str):
    for call in HASHFILES_RE.finditer(value):
        yield from QUOTED_ARG_RE.findall(call.group("args"))


def check_path_refs(workflow: dict, repo_root: Path, allowlist=None):
    """Return (problems, skipped) for a parsed workflow against repo_root."""
    allowlist = RUNTIME_PATH_ALLOWLIST if allowlist is None else allowlist
    problems: list[str] = []
    skipped: list[str] = []

    def check_one(location: str, path: str):
        path = path.strip()
        if not path or path in allowlist:
            return
        if any(c in path for c in GLOB_CHARS):
            if not glob.glob(str(repo_root / path), recursive=True):
                problems.append(f"{location}: glob {path!r} matches no files")
        elif not (repo_root / path).exists():
            problems.append(f"{location}: path {path!r} does not exist")

    for location, value in _workflow_path_refs(workflow):
        if "hashFiles(" in value:
            for call in HASHFILES_RE.finditer(value):
                raw_args = call.group("args")
                literals = QUOTED_ARG_RE.findall(raw_args)
                leftover = QUOTED_ARG_RE.sub("", raw_args).replace(",", "").strip()
                if leftover or not literals:
                    skipped.append(f"{location}: {call.group(0)!r} (non-literal hashFiles arguments)")
                for arg in literals:
                    check_one(f"{location} hashFiles", arg)
        elif "${{" in value:
            skipped.append(f"{location}: {value!r} (expression)")
        else:
            for line in value.splitlines():
                check_one(location, line)
    return problems, skipped


class WorkflowPathContractTests(unittest.TestCase):
    def test_every_workflow_path_exists(self):
        workflows = sorted(WORKFLOWS_DIR.glob("*.yml"))
        self.assertTrue(workflows, "no workflows found")
        problems: list[str] = []
        all_skipped: list[str] = []
        for wf in workflows:
            parsed = yaml.safe_load(wf.read_text(encoding="utf-8"))
            wf_problems, skipped = check_path_refs(parsed, REPO_ROOT)
            problems += [f"{wf.name}: {p}" for p in wf_problems]
            all_skipped += [f"{wf.name}: {s}" for s in skipped]
        self.assertFalse(problems, "\n" + "\n".join(problems))
        unexpected = sorted(set(all_skipped) - EXPECTED_SKIPS)
        self.assertFalse(
            unexpected,
            "path values skipped by the checker and not listed in EXPECTED_SKIPS:\n"
            + "\n".join(unexpected),
        )

    def test_drift_patrol_uses_shatter_go_module(self):
        parsed = yaml.safe_load(
            (WORKFLOWS_DIR / "drift-patrol.yml").read_text(encoding="utf-8")
        )
        refs = dict(_workflow_path_refs(parsed))
        values = {v for k, v in refs.items() if k.endswith("with.go-version-file")}
        self.assertEqual(values, {"shatter-go/go.mod"})
        cache = {v for k, v in refs.items() if k.endswith("with.cache-dependency-path")}
        self.assertIn("shatter-go/go.sum", cache)


class CheckPathRefsUnitTests(unittest.TestCase):
    def setUp(self):
        import tempfile

        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        self.root = Path(self._tmp.name)
        (self.root / "present").mkdir()
        (self.root / "present" / "Cargo.lock").write_text("")

    def wf(self, **step):
        return {"jobs": {"j": {"steps": [step]}}}

    def test_missing_literal_path_fails(self):
        problems, _ = check_path_refs(
            self.wf(uses="actions/setup-go@v5", **{"with": {"go-version-file": "go.mod"}}),
            self.root,
            {},
        )
        self.assertEqual(len(problems), 1)
        self.assertIn("go-version-file", problems[0])
        self.assertIn("'go.mod'", problems[0])

    def test_multiline_cache_dependency_path_checks_each_line(self):
        problems, _ = check_path_refs(
            self.wf(**{"with": {"cache-dependency-path": "present/Cargo.lock\nnope/x.lock\n"}}),
            self.root,
            {},
        )
        self.assertEqual(len(problems), 1)
        self.assertIn("nope/x.lock", problems[0])

    def test_glob_with_zero_matches_fails(self):
        problems, _ = check_path_refs(
            self.wf(**{"with": {"key": "k-${{ hashFiles('**/Missing.lock') }}"}}), self.root, {}
        )
        self.assertEqual(len(problems), 1)
        self.assertIn("matches no files", problems[0])

    def test_glob_with_matches_passes(self):
        problems, _ = check_path_refs(
            self.wf(**{"with": {"key": "k-${{ hashFiles('**/Cargo.lock', 'present/Cargo.lock') }}"}}),
            self.root,
            {},
        )
        self.assertEqual(problems, [])

    def test_expression_is_skipped_and_listed(self):
        problems, skipped = check_path_refs(
            self.wf(**{"working-directory": "${{ matrix.dir }}"}), self.root, {}
        )
        self.assertEqual(problems, [])
        self.assertEqual(len(skipped), 1)
        self.assertIn("matrix.dir", skipped[0])

    def test_non_literal_hashfiles_args_are_skipped_and_listed(self):
        problems, skipped = check_path_refs(
            self.wf(**{"with": {"key": 'k-${{ hashFiles("**/Missing.lock") }}'}}), self.root, {}
        )
        self.assertEqual(problems, [])
        self.assertEqual(len(skipped), 1)
        self.assertIn("non-literal hashFiles", skipped[0])

    def test_allowlisted_runtime_path_passes(self):
        wf = self.wf(**{"working-directory": "staging"})
        self.assertEqual(len(check_path_refs(wf, self.root, {})[0]), 1)
        self.assertEqual(check_path_refs(wf, self.root, {"staging": "created by the packaging step"})[0], [])


if __name__ == "__main__":
    unittest.main()
