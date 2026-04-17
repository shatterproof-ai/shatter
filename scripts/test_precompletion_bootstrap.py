from __future__ import annotations

import unittest
from pathlib import Path


REPO_ROOT = Path(__file__).resolve().parents[1]
TASKFILE_PATH = REPO_ROOT / "Taskfile.yml"
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

    def test_e2e_task_builds_typescript_frontend(self) -> None:
        e2e_block = read_task_block("e2e")
        self.assertIn("deps: [ts:build]", e2e_block)

    def test_pre_completion_skill_uses_bootstrapped_e2e_task(self) -> None:
        skill_text = PRE_COMPLETION_SKILL_PATH.read_text(encoding="utf-8")
        self.assertIn("npx task smoke", skill_text)
        self.assertIn("npx task e2e", skill_text)


if __name__ == "__main__":
    unittest.main()
