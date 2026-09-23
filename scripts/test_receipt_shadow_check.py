"""Tests for scripts/receipt-shadow-check.py (str-35vtk.25).

Covers: candidate/base tree computation (explicit remote SHA and the
brand-new-ref merge-base fallback), diff_class classification, the
multi-ref/deletion-only "no attempt at validation" cases, decision/reasons
sourced from the str-35vtk.23 validator (scripts/gate-receipt.py), the
classification matrix (false_accept/expected_miss/unexpected_miss/match),
event-log wiring (str-35vtk.18), and log-append-failure safety (the shadow
check must warn on stderr and still exit 0, never affecting a caller's exit
code).
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from scripts.git_sandbox_test_lib import sanitized_git_env

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "receipt-shadow-check.py"
GATE_RECEIPT = ROOT / "scripts" / "gate-receipt.py"

ZERO_SHA = "0" * 40


def _sha_from(text: str) -> str:
    """Deterministic-looking fake 40-hex-char SHA for lines that never need
    to resolve to a real object (e.g. an unrelated remote SHA in tests that
    exercise decision computation only through explicit inputs)."""
    import hashlib

    return hashlib.sha1(text.encode()).hexdigest()


class ShadowRepo:
    """Disposable Git repository plus the env/paths the shadow check needs."""

    def __init__(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        self.repo = self.root / "repo"
        self.runtime = self.root / "runtime"
        self.repo.mkdir()
        self.runtime.mkdir(mode=0o700)
        self.event_log = self.root / "gate-events.jsonl"

        self.env = sanitized_git_env()
        self.env["XDG_RUNTIME_DIR"] = str(self.runtime)

        self._git("init", "--quiet")
        self._git("config", "user.email", "shadow@example.test")
        self._git("config", "user.name", "Shadow Test")

        self._write("Taskfile.yml", "version: '3'\ntasks: {base: {}}\n")
        self._write("scripts/gate-wrapper.sh", "#!/bin/sh\nexit 0\n")
        self._write("scripts/gate-receipt.py", "#!/usr/bin/env python3\n# base\n")
        self._git("add", "-A")
        self._git("commit", "-q", "-m", "base")
        self.base_sha = self._git("rev-parse", "HEAD").stdout.strip()
        self.base_tree = self._git("rev-parse", "HEAD^{tree}").stdout.strip()

        # origin/main tracks the base commit, for the zero-remote-sha path.
        self._git("update-ref", "refs/remotes/origin/main", self.base_sha)

        self._write("src/app.py", "print('candidate')\n")
        self._git("add", "-A")
        self._git("commit", "-q", "-m", "candidate: other change")
        self.candidate_sha = self._git("rev-parse", "HEAD").stdout.strip()
        self.candidate_tree = self._git("rev-parse", "HEAD^{tree}").stdout.strip()

    def close(self) -> None:
        self._tmp.cleanup()

    def _git(self, *args: str) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            ["git", *args],
            cwd=self.repo,
            env=self.env,
            text=True,
            capture_output=True,
            check=True,
        )

    def _write(self, relative: str, text: str) -> None:
        path = self.repo / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def commit_beads_only(self) -> tuple[str, str]:
        """A second candidate whose only change from base is under .beads/**."""
        self._git("checkout", "-q", self.base_sha)
        self._write(".beads/issues.jsonl", '{"id":"x-1"}\n')
        self._git("add", "-A")
        self._git("commit", "-q", "-m", "beads only")
        sha = self._git("rev-parse", "HEAD").stdout.strip()
        tree = self._git("rev-parse", "HEAD^{tree}").stdout.strip()
        self._git("checkout", "-q", self.candidate_sha)
        return sha, tree

    def write_receipt(
        self,
        *,
        candidate: str,
        base: str,
        gate: str,
        argv: list[str],
        exit_code: int = 0,
    ) -> subprocess.CompletedProcess[str]:
        result_path = self.root / "gate-result.json"
        result_path.write_text(
            json.dumps(
                {
                    "gate": gate,
                    "argv": argv,
                    "started_at": "2026-09-23T12:00:00Z",
                    "ended_at": "2026-09-23T12:01:00Z",
                    "exit_code": exit_code,
                }
            )
        )
        return subprocess.run(
            [
                sys.executable,
                str(GATE_RECEIPT),
                "write",
                "--candidate",
                candidate,
                "--base",
                base,
                "--tier",
                "local",
                "--gate-result",
                str(result_path),
            ],
            cwd=self.repo,
            env=self.env,
            text=True,
            capture_output=True,
            check=False,
        )

    def run_shadow(
        self,
        stdin_lines: str,
        *,
        push_task: str = "",
        gate_ran: bool = False,
        gate_exit: int | None = None,
        event_log_path: Path | None = None,
        worktree: str | None = None,
    ) -> subprocess.CompletedProcess[str]:
        args = [
            sys.executable,
            str(SCRIPT),
            "--worktree",
            worktree or str(self.repo),
            "--push-task",
            push_task,
            "--gate-ran",
            "1" if gate_ran else "0",
            "--event-log-path",
            str(event_log_path or self.event_log),
        ]
        if gate_exit is not None:
            args.extend(["--gate-exit", str(gate_exit)])
        return subprocess.run(
            args,
            cwd=self.repo,
            env=self.env,
            input=stdin_lines,
            text=True,
            capture_output=True,
            check=False,
        )

    def events(self, path: Path | None = None) -> list[dict]:
        target = path or self.event_log
        if not target.exists():
            return []
        return [json.loads(line) for line in target.read_text().splitlines() if line.strip()]


def feature_line(repo: ShadowRepo, *, remote_sha: str | None = None) -> str:
    remote = remote_sha if remote_sha is not None else repo.base_sha
    return f"refs/heads/feature/x {repo.candidate_sha} refs/heads/feature/x {remote}"


def main_line(repo: ShadowRepo) -> str:
    return f"refs/heads/main {repo.candidate_sha} refs/heads/main {repo.base_sha}"


class ScriptExistsTest(unittest.TestCase):
    def test_script_exists(self) -> None:
        self.assertTrue(SCRIPT.is_file(), f"missing {SCRIPT}")


class SingleRefDecisionTest(unittest.TestCase):
    def setUp(self) -> None:
        self.fixture = ShadowRepo()
        self.addCleanup(self.fixture.close)

    def test_feature_branch_no_prior_receipt_is_invalid_missing_gate(self) -> None:
        result = self.fixture.run_shadow(
            feature_line(self.fixture) + "\n",
            push_task="affected",
            gate_ran=True,
            gate_exit=0,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        events = self.fixture.events()
        self.assertEqual(len(events), 1)
        payload = events[0]["payload"]
        self.assertEqual(payload["decision"], "invalid")
        self.assertIn("missing_gate", payload["reasons"])
        self.assertEqual(payload["candidate"], self.fixture.candidate_tree)
        self.assertEqual(payload["base"], self.fixture.base_tree)
        self.assertEqual(payload["diff_class"], "other")
        self.assertEqual(payload["classification"], "expected_miss")
        self.assertEqual(payload["real_gate"], {"gate": "task affected", "ran": True, "exit_code": 0})
        self.assertEqual(events[0]["event_type"], "receipt_shadow")
        self.assertEqual(events[0]["gate"], "task affected")

    def test_main_branch_with_matching_valid_receipt_is_reuse_match(self) -> None:
        write = self.fixture.write_receipt(
            candidate=self.fixture.candidate_tree,
            base=self.fixture.base_tree,
            gate="task check",
            argv=["task", "check"],
        )
        self.assertEqual(write.returncode, 0, write.stderr)

        result = self.fixture.run_shadow(
            main_line(self.fixture) + "\n",
            push_task="check",
            gate_ran=True,
            gate_exit=0,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = self.fixture.events()[0]["payload"]
        self.assertEqual(payload["decision"], "reuse")
        self.assertEqual(payload["reasons"], [])
        self.assertEqual(payload["classification"], "match")

    def test_matching_receipt_but_real_gate_failed_is_false_accept(self) -> None:
        write = self.fixture.write_receipt(
            candidate=self.fixture.candidate_tree,
            base=self.fixture.base_tree,
            gate="task check",
            argv=["task", "check"],
        )
        self.assertEqual(write.returncode, 0, write.stderr)

        result = self.fixture.run_shadow(
            main_line(self.fixture) + "\n",
            push_task="check",
            gate_ran=True,
            gate_exit=1,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = self.fixture.events()[0]["payload"]
        self.assertEqual(payload["decision"], "reuse")
        self.assertEqual(payload["classification"], "false_accept")

    def test_mismatched_receipt_argv_is_invalid_with_concrete_reason(self) -> None:
        write = self.fixture.write_receipt(
            candidate=self.fixture.candidate_tree,
            base=self.fixture.base_tree,
            gate="task check",
            argv=["task", "check", "--extra"],
        )
        self.assertEqual(write.returncode, 0, write.stderr)

        result = self.fixture.run_shadow(
            main_line(self.fixture) + "\n",
            push_task="check",
            gate_ran=True,
            gate_exit=0,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = self.fixture.events()[0]["payload"]
        self.assertEqual(payload["decision"], "invalid")
        self.assertIn("argv", payload["reasons"])
        self.assertEqual(payload["classification"], "expected_miss")

    def test_new_branch_zero_remote_sha_uses_merge_base_with_origin_main(self) -> None:
        result = self.fixture.run_shadow(
            feature_line(self.fixture, remote_sha=ZERO_SHA) + "\n",
            push_task="affected",
            gate_ran=True,
            gate_exit=0,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = self.fixture.events()[0]["payload"]
        # merge-base(candidate, origin/main==base) == base here, so the
        # base tree resolved must equal the base commit's own tree.
        self.assertEqual(payload["base"], self.fixture.base_tree)
        self.assertEqual(payload["candidate"], self.fixture.candidate_tree)

    def test_beads_only_diff_classifies_as_beads_only(self) -> None:
        beads_sha, tree = self.fixture.commit_beads_only()
        line = f"refs/heads/feature/beads {beads_sha} refs/heads/feature/beads {self.fixture.base_sha}"
        result = self.fixture.run_shadow(
            line + "\n", push_task="affected", gate_ran=True, gate_exit=0
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = self.fixture.events()[0]["payload"]
        self.assertEqual(payload["diff_class"], "beads_only")
        self.assertEqual(payload["base"], self.fixture.base_tree)
        self.assertEqual(payload["candidate"], tree)

    def test_no_gate_required_push_task_empty(self) -> None:
        result = self.fixture.run_shadow(
            feature_line(self.fixture) + "\n",
            push_task="",
            gate_ran=False,
            gate_exit=None,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = self.fixture.events()[0]["payload"]
        self.assertEqual(payload["decision"], "no_gate")
        self.assertIsNone(payload["requirements"])
        self.assertEqual(payload["classification"], "match")
        self.assertEqual(self.fixture.events()[0]["gate"], "none")


class MultiRefAndDeletionTest(unittest.TestCase):
    def setUp(self) -> None:
        self.fixture = ShadowRepo()
        self.addCleanup(self.fixture.close)

    def test_multiple_non_deletion_refs_is_invalid_multi_ref(self) -> None:
        lines = "\n".join(
            [
                feature_line(self.fixture),
                main_line(self.fixture),
            ]
        )
        result = self.fixture.run_shadow(
            lines + "\n", push_task="check", gate_ran=True, gate_exit=0
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = self.fixture.events()[0]["payload"]
        self.assertEqual(payload["decision"], "invalid")
        self.assertEqual(payload["reasons"], ["multi_ref"])
        self.assertEqual(payload["diff_class"], "unknown")
        self.assertIsNone(payload["candidate"])
        self.assertIsNone(payload["base"])
        self.assertEqual(payload["classification"], "expected_miss")
        # Both ref updates are still recorded in full.
        self.assertEqual(len(payload["update"]), 2)

    def test_deletion_only_push_is_invalid_no_target_ref(self) -> None:
        line = f"(delete) {ZERO_SHA} refs/heads/feature/gone {self.fixture.base_sha}"
        result = self.fixture.run_shadow(
            line + "\n", push_task="", gate_ran=False, gate_exit=None
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = self.fixture.events()[0]["payload"]
        self.assertEqual(payload["decision"], "invalid")
        self.assertEqual(payload["reasons"], ["no_target_ref"])
        self.assertEqual(payload["classification"], "expected_miss")

    def test_multi_ref_push_id_is_unique_per_invocation(self) -> None:
        line = feature_line(self.fixture)
        r1 = self.fixture.run_shadow(line + "\n", push_task="affected", gate_ran=True, gate_exit=0)
        r2 = self.fixture.run_shadow(line + "\n", push_task="affected", gate_ran=True, gate_exit=0)
        self.assertEqual(r1.returncode, 0, r1.stderr)
        self.assertEqual(r2.returncode, 0, r2.stderr)
        events = self.fixture.events()
        self.assertEqual(len(events), 2)
        self.assertNotEqual(events[0]["payload"]["push_id"], events[1]["payload"]["push_id"])


class EventLogFailureSafetyTest(unittest.TestCase):
    def setUp(self) -> None:
        self.fixture = ShadowRepo()
        self.addCleanup(self.fixture.close)

    def test_unwritable_event_log_path_only_warns_and_exits_zero(self) -> None:
        # A path whose parent is actually a regular file can never be
        # created as a directory by gate-event-log.py -- a deterministic,
        # portable way (no chmod/uid tricks) to force its append to fail.
        blocker = self.fixture.root / "not-a-directory"
        blocker.write_text("blocker\n")
        bad_log_path = blocker / "gate-events.jsonl"

        result = self.fixture.run_shadow(
            feature_line(self.fixture) + "\n",
            push_task="affected",
            gate_ran=True,
            gate_exit=0,
            event_log_path=bad_log_path,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("warning", result.stderr.lower())
        self.assertFalse(bad_log_path.exists())


class PureFunctionTest(unittest.TestCase):
    """Direct tests of the module's pure helpers, without spawning a repo."""

    def setUp(self) -> None:
        sys.path.insert(0, str(ROOT / "scripts"))
        import importlib.util

        spec = importlib.util.spec_from_file_location("receipt_shadow_check", SCRIPT)
        assert spec is not None and spec.loader is not None
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        self.module = module

    def test_parse_ref_lines_rejects_malformed_line(self) -> None:
        with self.assertRaises(ValueError):
            self.module.parse_ref_lines("only three fields\n")

    def test_parse_ref_lines_skips_blank_lines(self) -> None:
        text = "\n\nrefs/heads/a sha1 refs/heads/a sha2\n\n"
        updates = self.module.parse_ref_lines(
            text.replace("sha1", "a" * 40).replace("sha2", "b" * 40)
        )
        self.assertEqual(len(updates), 1)

    def test_non_deletion_updates_filters_zero_sha(self) -> None:
        updates = [
            {"local_ref": "r", "local_sha": ZERO_SHA, "remote_ref": "r", "remote_sha": "a" * 40},
            {"local_ref": "r2", "local_sha": "b" * 40, "remote_ref": "r2", "remote_sha": "c" * 40},
        ]
        self.assertEqual(len(self.module.non_deletion_updates(updates)), 1)

    def test_build_requirements_empty_push_task_is_none(self) -> None:
        self.assertIsNone(self.module.build_requirements(""))

    def test_build_requirements_shape(self) -> None:
        req = self.module.build_requirements("check")
        self.assertEqual(
            req,
            {"schema": 1, "requirements": [{"gate": "task check", "argv": ["task", "check"]}]},
        )

    def test_classification_matrix(self) -> None:
        cc = self.module.compute_classification
        self.assertEqual(cc("reuse", [], True, 0), "match")
        self.assertEqual(cc("reuse", [], True, 1), "false_accept")
        self.assertEqual(cc("reuse", [], False, None), "match")
        self.assertEqual(cc("invalid", ["missing_gate"], True, 0), "expected_miss")
        self.assertEqual(cc("invalid", [], True, 0), "unexpected_miss")
        self.assertEqual(cc("no_gate", [], False, None), "match")

    def test_is_normalized_abs_path(self) -> None:
        self.assertTrue(self.module.is_normalized_abs_path("/a/b"))
        self.assertFalse(self.module.is_normalized_abs_path("relative/path"))
        self.assertFalse(self.module.is_normalized_abs_path("/a/b/"))
        self.assertFalse(self.module.is_normalized_abs_path("/a/../b"))


if __name__ == "__main__":
    unittest.main()
