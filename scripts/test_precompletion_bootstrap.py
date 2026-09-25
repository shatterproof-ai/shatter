from __future__ import annotations

import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
TASKFILE_PATH = REPO_ROOT / "Taskfile.yml"
CORE_TASKFILE_PATH = REPO_ROOT / "shatter-core" / "Taskfile.yml"
PRE_COMPLETION_SKILL_PATH = REPO_ROOT / ".claude" / "skills" / "pre-completion" / "SKILL.md"


def read_task_block(task_name: str) -> str:
    lines = TASKFILE_PATH.read_text(encoding="utf-8").splitlines()
    block_lines: list[str] = []
    in_block = False

    for line in lines:
        if line.startswith("  ") and line.endswith(":") and not line.startswith("    "):
            current_name = line.strip()[:-1]
            if in_block and current_name != task_name:
                break
            in_block = current_name == task_name
        if in_block:
            block_lines.append(line)

    if not block_lines:
        raise AssertionError(f"task {task_name!r} not found in {TASKFILE_PATH}")

    return "\n".join(block_lines)


class PreCompletionBootstrapTest(unittest.TestCase):
    def test_smoke_task_builds_typescript_frontend(self) -> None:
        smoke_block = read_task_block("smoke")
        self.assertIn("deps: [ts:build]", smoke_block)

    def test_e2e_governed_dag_builds_typescript_frontend(self) -> None:
        e2e_block = read_task_block("e2e-governed")
        self.assertIn("deps: [ts:build, go:build, rust-fe:build]", e2e_block)
        self.assertIn("task: e2e-ts", e2e_block)

    def test_pre_completion_uses_affected_gates_without_unconditional_e2e(self) -> None:
        skill_text = PRE_COMPLETION_SKILL_PATH.read_text(encoding="utf-8")
        self.assertIn("scripts/affected-gates.py", skill_text)
        self.assertIn("task affected", skill_text)
        self.assertIn("Gates selected", skill_text)
        self.assertNotIn("task e2e\n", skill_text)

    def test_pre_completion_task_executes_affected_selection(self) -> None:
        block = read_task_block("pre-completion")
        self.assertIn("task: affected", block)
        self.assertNotIn("task: check", block)

    def test_full_pre_completion_runs_frontend_e2e_once(self) -> None:
        block = read_task_block("pre-completion-e2e")
        self.assertEqual(block.count("- task: check"), 1)
        self.assertNotIn("- task: e2e", block)

        integration = read_task_block("check-integration")
        for frontend in ("ts:build", "go:build", "rust-fe:build"):
            self.assertIn(f"- {frontend}", integration)

        core_taskfile = CORE_TASKFILE_PATH.read_text(encoding="utf-8")
        for source in (
            "../shatter-ts/src/**/*.ts",
            "../shatter-go/**/*.go",
            "../shatter-rust/src/**/*.rs",
            "../shatter-rust-runtime/src/**/*.rs",
        ):
            self.assertEqual(core_taskfile.count(f"- {source}"), 2)


if __name__ == "__main__":
    unittest.main()
