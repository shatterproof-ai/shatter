#!/usr/bin/env python3
"""Regression tests for heavyweight Task DAG admission (str-35vtk.11)."""

from __future__ import annotations

import os
from pathlib import Path
import re
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
    "affected": ("affected", "affected-governed"),
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
CACHED_GOVERNED_TASKS = {
    "conformance",
    "parity",
    "golden-test",
    "e2e-ts",
    "e2e-go",
    "e2e-rust",
}
GATE_BUDGETS = {
    "check-fast": {
        "PROPTEST_CASES": "32",
        "SHATTER_FUZZ_CASES": "32",
    },
    "check": {
        "PROPTEST_CASES": "256",
        "SHATTER_FUZZ_CASES": "1000",
        "SHATTER_FAST_CHECK_NUM_RUNS": "default",
    },
}
TASK_NAME_TOKEN = re.compile(r"[A-Za-z0-9_][A-Za-z0-9_-]*")


def load_tasks() -> dict[str, dict]:
    document = yaml.safe_load(TASKFILE.read_text())
    return document["tasks"]


def command_text(command: object) -> str | None:
    return command if isinstance(command, str) else None


def command_references(command: object) -> list[str]:
    if isinstance(command, str):
        return TASK_NAME_TOKEN.findall(command)
    if isinstance(command, list):
        return [reference for item in command for reference in command_references(item)]
    if isinstance(command, dict):
        return [
            reference
            for key in ("task", "cmd", "defer")
            if key in command
            for reference in command_references(command[key])
        ]
    return []


def command_strings(command: object) -> list[str]:
    if isinstance(command, str):
        return [command]
    if isinstance(command, list):
        return [text for item in command for text in command_strings(item)]
    if isinstance(command, dict):
        return [
            text
            for key in ("cmd", "defer")
            if key in command
            for text in command_strings(command[key])
        ]
    return []


def task_references(task: dict) -> list[str]:
    return command_references(task.get("deps", [])) + command_references(
        [task.get("cmd"), task.get("cmds", [])]
    )


def wrapper_commands(task: dict) -> list[str]:
    return [
        text
        for text in command_strings([task.get("cmd"), task.get("cmds", [])])
        if "scripts/gate-wrapper.sh" in text
    ]


class GovernedTaskGraphTests(unittest.TestCase):
    def test_task_reference_scan_handles_flags_maps_and_unbalanced_shell(self) -> None:
        cases = {
            "string dependency": ({"deps": ["dep-string-governed"]}, "dep-string-governed"),
            "map dependency": (
                {"deps": [{"task": "dep-map-governed", "vars": {"MODE": "full"}}]},
                "dep-map-governed",
            ),
            "flags": ({"cmds": ["task --force flags-governed"]}, "flags-governed"),
            "global option": (
                {"cmds": ["task --dir . global-option-governed"]},
                "global-option-governed",
            ),
            "task map": ({"cmds": [{"task": "task-map-governed"}]}, "task-map-governed"),
            "command map": (
                {"cmds": [{"cmd": "task command-map-governed", "silent": True}]},
                "command-map-governed",
            ),
            "deferred command": (
                {"cmds": [{"defer": "task deferred-governed"}]},
                "deferred-governed",
            ),
            "unbalanced shell": (
                {"cmds": ["sh -c 'task unbalanced-governed"]},
                "unbalanced-governed",
            ),
            "top-level command": (
                {"cmd": {"cmd": "task top-level-governed"}},
                "top-level-governed",
            ),
        }
        for syntax, (task, expected) in cases.items():
            with self.subTest(syntax=syntax):
                self.assertIn(expected, task_references(task))

    def test_wrapper_scan_handles_string_map_and_top_level_commands(self) -> None:
        wrapper = "bash scripts/gate-wrapper.sh check task check-governed"
        cases = {
            "string": {"cmds": [wrapper]},
            "command map": {"cmds": [{"cmd": wrapper}]},
            "top-level command": {"cmd": {"cmd": wrapper}},
        }
        for syntax, task in cases.items():
            with self.subTest(syntax=syntax):
                self.assertEqual(wrapper_commands(task), [wrapper])

    def test_every_public_spelling_wraps_one_hidden_full_dag(self) -> None:
        tasks = load_tasks()
        implementation_names = {
            implementation for _, implementation in GOVERNED_TASKS.values()
        }
        wrapper_owners: set[str] = set()
        spellings_seen: dict[str, str] = {}

        for public, (label, implementation) in GOVERNED_TASKS.items():
            with self.subTest(task=public):
                public_task = tasks[public]
                self.assertNotIn("deps", public_task)
                self.assertEqual(public_task.get("env", {}), GATE_BUDGETS.get(public, {}))
                self.assertEqual(len(public_task.get("cmds", [])), 1)
                self.assertEqual(
                    self._wrapper_argv(public_task["cmds"][0]),
                    ["bash", "scripts/gate-wrapper.sh", label, "task", implementation],
                )
                wrapper_owners.add(public)

                implementation_task = tasks[implementation]
                # The wrapper invokes its implementation through a fresh task
                # CLI process, which cannot enter internal:true tasks. Omit
                # desc/aliases instead so it stays off the normal task list.
                self.assertNotIn("desc", implementation_task)
                self.assertNotIn("aliases", implementation_task)
                self.assertNotIn("internal", implementation_task)
                self.assertNotIn("sources", implementation_task)
                self.assertNotIn("preconditions", implementation_task)
                self.assertNotIn("env", implementation_task)
                self.assertTrue(
                    implementation_task.get("deps") or implementation_task.get("cmds"),
                    f"{implementation} does not own any work",
                )

                for spelling in [public, *public_task.get("aliases", [])]:
                    self.assertNotIn(spelling, spellings_seen)
                    spellings_seen[spelling] = public

        for task_name, task in tasks.items():
            wrappers = wrapper_commands(task)
            if wrappers:
                self.assertIn(task_name, GOVERNED_TASKS)
                self.assertEqual(len(wrappers), 1)

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
        self.assertTrue(
            all(tasks[task_name].get("sources") for task_name in CACHED_GOVERNED_TASKS)
        )

    def test_public_gate_budgets_reach_nested_stages(self) -> None:
        tasks = load_tasks()
        with tempfile.TemporaryDirectory() as temp_dir:
            fixture = Path(temp_dir)
            self._write_runtime_fixture_tools(fixture)
            fixture_tasks: dict[str, dict] = {}
            for public, expected in GATE_BUDGETS.items():
                implementation = GOVERNED_TASKS[public][1]
                fixture_tasks[public] = {
                    "env": tasks[public].get("env", {}),
                    "cmds": [tasks[public]["cmds"][0]],
                }
                fixture_tasks[implementation] = {"cmds": [{"task": f"probe-{public}"}]}
                values = " ".join(f"{name}=${{{name}:-missing}}" for name in expected)
                fixture_tasks[f"probe-{public}"] = {
                    "cmds": [f'echo "{public} {values}" >> "$EVENT_LOG"']
                }
            (fixture / "Taskfile.yml").write_text(
                yaml.safe_dump({"version": "3", "tasks": fixture_tasks}, sort_keys=False)
            )

            event_log = fixture / "events.log"
            environment = self._fixture_environment(fixture, event_log)
            for public, expected in GATE_BUDGETS.items():
                with self.subTest(task=public):
                    event_log.unlink(missing_ok=True)
                    self._run_task(fixture, environment, public)
                    event_text = event_log.read_text()
                    for name, value in expected.items():
                        self.assertIn(f"{name}={value}", event_text)

    def test_lease_is_acquired_before_every_injected_prerequisite(self) -> None:
        self.assertIsNotNone(shutil.which("task"), "task is required")
        self.assertIsNotNone(shutil.which("flock"), "flock is required")

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
                with self.subTest(task=public):
                    event_log.unlink(missing_ok=True)
                    self._run_task(fixture, environment, public, force=False)
                    event_text = event_log.read_text()
                    events = event_text.splitlines()
                    self.assertEqual(events[0], "lease-acquired")
                    self.assertEqual(events.count("lease-acquired"), 1)
                    for suffix in ("a", "b"):
                        prerequisite = f"prerequisite-start:{public}:{suffix}:lock=1"
                        self.assertEqual(event_text.count(prerequisite), 1, events)
                        self.assertLess(
                            event_text.index("lease-acquired"),
                            event_text.index(prerequisite),
                        )

    def test_nested_governed_task_acquires_one_lease(self) -> None:
        self.assertIsNotNone(shutil.which("task"), "task is required")
        self.assertIsNotNone(shutil.which("flock"), "flock is required")

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

    def test_unchanged_cached_gate_does_not_acquire_a_lease(self) -> None:
        self.assertIsNotNone(shutil.which("task"), "task is required")
        self.assertIsNotNone(shutil.which("flock"), "flock is required")

        tasks = load_tasks()
        with tempfile.TemporaryDirectory() as temp_dir:
            fixture = Path(temp_dir)
            self._write_runtime_fixture_tools(fixture)
            source = fixture / "input.txt"
            source.write_text("first\n")
            lines = ["version: '3'", "tasks:"]
            for public in sorted(CACHED_GOVERNED_TASKS):
                implementation = GOVERNED_TASKS[public][1]
                wrapper = command_text(tasks[public]["cmds"][0])
                self._wrapper_argv(wrapper)
                lines.extend(
                    [
                        f"  {public}:",
                        "    sources: [input.txt]",
                        "    cmds:",
                        "      - |-",
                        f"          {wrapper}",
                        f"  {implementation}:",
                        "    cmds:",
                        "      - |-",
                        f"          echo body:{public} >> \"$EVENT_LOG\"",
                    ]
                )
            (fixture / "Taskfile.yml").write_text("\n".join(lines) + "\n")

            event_log = fixture / "events.log"
            environment = self._fixture_environment(fixture, event_log)
            for public in sorted(CACHED_GOVERNED_TASKS):
                with self.subTest(task=public):
                    event_log.unlink(missing_ok=True)
                    self._run_task(fixture, environment, public, force=False)
                    first_events = event_log.read_text().splitlines()
                    self.assertEqual(first_events.count("lease-acquired"), 1)
                    self.assertIn(f"body:{public}", first_events)

                    self._run_task(fixture, environment, public, force=False)
                    self.assertEqual(event_log.read_text().splitlines(), first_events)

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
    def _run_task(
        fixture: Path,
        environment: dict[str, str],
        task_name: str,
        *,
        force: bool = True,
    ) -> None:
        completed = subprocess.run(
            ["task", *(["--force"] if force else []), task_name],
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
        environment = os.environ | {
            "EVENT_LOG": str(event_log),
            "HOME": str(fixture / "home"),
            "PATH": f"{fixture / 'bin'}:{os.environ['PATH']}",
            "SHATTER_HEAVY_SLOTS": "1",
            "XDG_RUNTIME_DIR": str(fixture / "runtime"),
        }
        # meta runs both directly and nested under the production check lease.
        # The fixture models a fresh top-level invocation in either context.
        environment.pop("SHATTER_GATE_LOCK_HELD", None)
        # Public gate budgets must win over any full-gate values inherited by
        # meta or CI; the fixture tests each facade as a fresh invocation.
        for name in {name for budgets in GATE_BUDGETS.values() for name in budgets}:
            environment.pop(name, None)
        return environment

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
