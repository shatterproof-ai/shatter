#!/usr/bin/env python3
"""Regression tests for heavyweight Task DAG admission (str-35vtk.11)."""

from __future__ import annotations

import os
from pathlib import Path
import shlex
import shutil
import subprocess
import tempfile
import unittest

import yaml


ROOT = Path(__file__).resolve().parents[1]
TASKFILE = ROOT / "Taskfile.yml"
# Public task -> (timing label, callable implementation task).  golden-test's
# historical timing label is "golden" and remains stable for CSV consumers.
GOVERNED_TASKS = {
    "check": ("check", "check-governed"),
    "check-fast": ("check-fast", "check-fast-governed"),
    "conformance": ("conformance", "conformance-governed"),
    "parity": ("parity", "parity-governed"),
    "golden-test": ("golden", "golden-test-governed"),
    "e2e": ("e2e", "e2e-governed"),
    "e2e-ts": ("e2e-ts", "e2e-ts-governed"),
    "e2e-go": ("e2e-go", "e2e-go-governed"),
    "e2e-rust": ("e2e-rust", "e2e-rust-governed"),
    "walkthrough": ("walkthrough", "walkthrough-governed"),
    "walkthrough-cold": ("walkthrough-cold", "walkthrough-cold-governed"),
    "gauntlet": ("gauntlet", "gauntlet-governed"),
    "gauntlet-cold": ("gauntlet-cold", "gauntlet-cold-governed"),
}


def load_tasks() -> dict[str, dict]:
    document = yaml.safe_load(TASKFILE.read_text())
    return document["tasks"]


def command_text(command: object) -> str | None:
    return command if isinstance(command, str) else None


def task_references(task: dict) -> list[str]:
    references: list[str] = list(task.get("deps", []))
    for command in task.get("cmds", []):
        if isinstance(command, dict) and isinstance(command.get("task"), str):
            references.append(command["task"])
        text = command_text(command)
        if text:
            words = shlex.split(text)
            references.extend(
                words[index + 1]
                for index, word in enumerate(words[:-1])
                if word == "task"
            )
    return references


class GovernedTaskGraphTests(unittest.TestCase):
    def test_every_public_spelling_wraps_one_hidden_full_dag(self) -> None:
        tasks = load_tasks()
        implementation_names = {implementation for _, implementation in GOVERNED_TASKS.values()}
        wrapper_owners: set[str] = set()
        spellings_seen: dict[str, str] = {}

        for public, (label, implementation) in GOVERNED_TASKS.items():
            with self.subTest(task=public):
                public_task = tasks[public]
                for key in ("deps", "preconditions", "sources", "env"):
                    self.assertNotIn(key, public_task)
                self.assertEqual(len(public_task.get("cmds", [])), 1)
                self.assertEqual(
                    self._wrapper_argv(public_task["cmds"][0]),
                    ["bash", "scripts/gate-wrapper.sh", label, "task", implementation],
                )
                wrapper_owners.add(public)

                implementation_task = tasks[implementation]
                # go-task refuses shell entry into internal:true tasks, so the
                # callable implementation is hidden by omitting desc/aliases
                # and guarded against direct, unleased invocation.
                self.assertNotIn("desc", implementation_task)
                self.assertNotIn("aliases", implementation_task)
                self.assertNotIn("internal", implementation_task)
                guards = "\n".join(
                    condition.get("sh", "")
                    for condition in implementation_task.get("preconditions", [])
                )
                self.assertIn("SHATTER_GATE_LOCK_HELD", guards)
                self.assertTrue(
                    implementation_task.get("deps") or implementation_task.get("cmds"),
                    f"{implementation} does not own any work",
                )

                for spelling in [public, *public_task.get("aliases", [])]:
                    self.assertNotIn(spelling, spellings_seen)
                    spellings_seen[spelling] = public

        for task_name, task in tasks.items():
            wrapper_commands = [
                command
                for command in task.get("cmds", [])
                if "scripts/gate-wrapper.sh" in (command_text(command) or "")
            ]
            if wrapper_commands:
                self.assertIn(task_name, GOVERNED_TASKS)
                self.assertEqual(len(wrapper_commands), 1)

            escaped = implementation_names.intersection(task_references(task))
            allowed = (
                {GOVERNED_TASKS[task_name][1]}
                if task_name in GOVERNED_TASKS
                else set()
            )
            self.assertEqual(
                escaped,
                allowed,
                f"{task_name} bypasses a public heavyweight wrapper",
            )

        self.assertEqual(wrapper_owners, set(GOVERNED_TASKS))

    def test_lease_is_acquired_before_every_injected_prerequisite(self) -> None:
        if shutil.which("task") is None or shutil.which("flock") is None:
            self.skipTest("task and flock are required")

        tasks = load_tasks()
        with tempfile.TemporaryDirectory() as temp_dir:
            fixture = Path(temp_dir)
            self._write_runtime_fixture_tools(fixture)
            lines = ["version: '3'", "tasks:"]
            for public, (_label, implementation) in GOVERNED_TASKS.items():
                wrapper = command_text(tasks[public]["cmds"][0])
                self._wrapper_argv(wrapper)
                lines.extend(
                    [
                        f"  {public}:",
                        "    cmds:",
                        "      - |-",
                        f"          {wrapper}",
                        f"  {implementation}:",
                        f"    deps: [probe-a-{public}, probe-b-{public}]",
                        f"  probe-a-{public}:",
                        "    cmds:",
                        "      - |-",
                        f"          echo prerequisite-start:{public}:a:lock=${{SHATTER_GATE_LOCK_HELD:-}} >> \"$EVENT_LOG\"",
                        f"  probe-b-{public}:",
                        "    cmds:",
                        "      - |-",
                        f"          echo prerequisite-start:{public}:b:lock=${{SHATTER_GATE_LOCK_HELD:-}} >> \"$EVENT_LOG\"",
                    ]
                )
            (fixture / "Taskfile.yml").write_text("\n".join(lines) + "\n")

            event_log = fixture / "events.log"
            environment = self._fixture_environment(fixture, event_log)
            for public in GOVERNED_TASKS:
                event_log.unlink(missing_ok=True)
                self._run_task(fixture, environment, public)
                events = event_log.read_text().splitlines()
                self.assertEqual(events[0], "lease-acquired")
                self.assertEqual(events.count("lease-acquired"), 1)
                prerequisites = [event for event in events if event.startswith("prerequisite-start:")]
                self.assertEqual(len(prerequisites), 2)
                self.assertTrue(all(event.endswith(":lock=1") for event in prerequisites))

    def test_nested_governed_task_acquires_one_lease(self) -> None:
        if shutil.which("task") is None or shutil.which("flock") is None:
            self.skipTest("task and flock are required")

        tasks = load_tasks()
        with tempfile.TemporaryDirectory() as temp_dir:
            fixture = Path(temp_dir)
            self._write_runtime_fixture_tools(fixture)
            outer = command_text(tasks["e2e"]["cmds"][0])
            inner = command_text(tasks["e2e-ts"]["cmds"][0])
            self._wrapper_argv(outer)
            self._wrapper_argv(inner)
            (fixture / "Taskfile.yml").write_text(
                f"""version: '3'
tasks:
  e2e:
    cmds:
      - |-
          {outer}
  e2e-governed:
    deps: [outer-prerequisite]
    cmds:
      - task: e2e-ts
  outer-prerequisite:
    cmds:
      - echo outer-prerequisite:lock=${{SHATTER_GATE_LOCK_HELD:-}} >> "$EVENT_LOG"
  e2e-ts:
    cmds:
      - |-
          {inner}
  e2e-ts-governed:
    deps: [inner-prerequisite]
    cmds:
      - echo inner-body:lock=${{SHATTER_GATE_LOCK_HELD:-}} >> "$EVENT_LOG"
  inner-prerequisite:
    cmds:
      - echo inner-prerequisite:lock=${{SHATTER_GATE_LOCK_HELD:-}} >> "$EVENT_LOG"
"""
            )

            event_log = fixture / "events.log"
            environment = self._fixture_environment(fixture, event_log)
            self._run_task(fixture, environment, "e2e")
            events = event_log.read_text().splitlines()

        self.assertEqual(events.count("lease-acquired"), 1)
        self.assertEqual(events[0], "lease-acquired")
        self.assertIn("outer-prerequisite:lock=1", events)
        self.assertIn("inner-prerequisite:lock=1", events)
        self.assertIn("inner-body:lock=1", events)

    @staticmethod
    def _wrapper_argv(command: object) -> list[str]:
        text = command_text(command)
        if text is None:
            raise AssertionError(f"wrapper command is not a string: {command!r}")
        argv = shlex.split(text)
        if len(argv) != 5 or argv[:2] != ["bash", "scripts/gate-wrapper.sh"]:
            raise AssertionError(f"not a facade wrapper command: {text!r}")
        return argv

    @staticmethod
    def _run_task(fixture: Path, environment: dict[str, str], task_name: str) -> None:
        completed = subprocess.run(
            ["task", "--force", task_name],
            cwd=fixture,
            env=environment,
            text=True,
            capture_output=True,
        )
        if completed.returncode:
            raise AssertionError(
                f"task {task_name} failed:\nstdout:\n{completed.stdout}\nstderr:\n{completed.stderr}"
            )

    @staticmethod
    def _fixture_environment(fixture: Path, event_log: Path) -> dict[str, str]:
        return os.environ | {
            "EVENT_LOG": str(event_log),
            "HOME": str(fixture / "home"),
            "PATH": f"{fixture / 'bin'}:{os.environ['PATH']}",
            "SHATTER_HEAVY_SLOTS": "1",
            "XDG_RUNTIME_DIR": str(fixture / "runtime"),
        }

    @staticmethod
    def _write_runtime_fixture_tools(fixture: Path) -> None:
        scripts = fixture / "scripts"
        scripts.mkdir()
        shutil.copy2(ROOT / "scripts/gate-wrapper.sh", scripts / "gate-wrapper.sh")

        fake_flock = fixture / "bin" / "flock"
        fake_flock.parent.mkdir()
        fake_flock.write_text(
            "#!/usr/bin/env bash\n"
            "if [[ ${1:-} == -n ]]; then\n"
            "  echo lease-acquired >> \"$EVENT_LOG\"\n"
            "fi\n"
            "exec /usr/bin/flock \"$@\"\n"
        )
        fake_flock.chmod(0o755)


if __name__ == "__main__":
    unittest.main()
