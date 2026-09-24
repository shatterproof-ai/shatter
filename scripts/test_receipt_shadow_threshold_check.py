"""Tests for scripts/receipt-shadow-threshold-check.py (str-35vtk.26).

Builds a synthetic fixture gate-event log (never the real production log --
see the module docstring and str-35vtk.26's scope note) plus a small
disposable git repo to exercise the remote-ancestry rule, and covers:

  - dedup by candidate tree
  - diff_class=beads_only exclusion
  - events whose local commit is NOT an ancestor of origin/main (failed /
    unreachable / abandoned pushes) exclusion
  - multi-ref push exclusion
  - deterministic first-10 selection in timestamp order
  - a false_accept in the selected set -> milestone NOT met
  - an unexplained unexpected_miss -> milestone NOT met
  - an unexpected_miss WITH a linked issue -> explained, milestone can still
    be met
  - the durable-note-append path, with the real `bd note` call stubbed out
"""

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from scripts.git_sandbox_test_lib import sanitized_git_env

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts" / "receipt-shadow-threshold-check.py"

ZERO_SHA = "0" * 40


def _load_module():
    spec = importlib.util.spec_from_file_location("receipt_shadow_threshold_check", SCRIPT)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


MODULE = _load_module()


class FixtureRepo:
    """A tiny disposable git repo used only to exercise the remote-ancestry
    rule: some commits are folded into origin/main, others are left on an
    abandoned branch that origin/main never merges."""

    def __init__(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.root = Path(self._tmp.name)
        self.repo = self.root / "repo"
        self.repo.mkdir()
        self.env = sanitized_git_env()

        self._git("init", "--quiet")
        self._git("config", "user.email", "fixture@example.test")
        self._git("config", "user.name", "Fixture Test")

        self._write("README.md", "base\n")
        self._git("add", "-A")
        self._git("commit", "-q", "-m", "base")
        self.base_sha = self._out("rev-parse", "HEAD")
        self._git("update-ref", "refs/remotes/origin/main", self.base_sha)
        self._git("update-ref", "refs/remotes/origin/master", self.base_sha)

    def close(self) -> None:
        self._tmp.cleanup()

    def _git(self, *args: str) -> None:
        subprocess.run(
            ["git", *args], cwd=self.repo, env=self.env, check=True,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )

    def _out(self, *args: str) -> str:
        result = subprocess.run(
            ["git", *args], cwd=self.repo, env=self.env, check=True,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        return result.stdout.strip()

    def _write(self, relative: str, text: str) -> None:
        path = self.repo / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def commit_and_land(self, name: str, remote: str = "origin/main") -> tuple[str, str]:
        """Commits a change and folds it into the given remote-tracking ref
        (default origin/main) -- an eligible, landed push."""
        self._write(f"{name}.txt", f"{name}\n")
        self._git("add", "-A")
        self._git("commit", "-q", "-m", name)
        sha = self._out("rev-parse", "HEAD")
        tree = self._out("rev-parse", "HEAD^{tree}")
        self._git("update-ref", f"refs/remotes/{remote}", sha)
        return sha, tree

    def commit_abandoned(self, name: str) -> tuple[str, str]:
        """Commits a change on top of current HEAD but never merges it into
        origin/main -- a failed/unreachable/abandoned push."""
        base = self._out("rev-parse", "HEAD")
        self._write(f"{name}.txt", f"{name}\n")
        self._git("add", "-A")
        self._git("commit", "-q", "-m", name)
        sha = self._out("rev-parse", "HEAD")
        tree = self._out("rev-parse", "HEAD^{tree}")
        # Roll HEAD back so later landed commits don't descend from this one.
        self._git("reset", "-q", "--hard", base)
        return sha, tree


def make_event(
    *,
    push_id: str,
    timestamp: str,
    local_sha: str,
    candidate: str,
    decision: str,
    classification: str,
    reasons: list[str] | None = None,
    diff_class: str = "other",
    remote_ref: str = "refs/heads/main",
    extra_updates: list[dict] | None = None,
    real_gate_exit_code: int | None = 0,
    real_gate_ran: bool = True,
    base: str = "deadbeef" * 5,
) -> dict:
    update = [
        {
            "local_ref": "refs/heads/main",
            "local_sha": local_sha,
            "remote_ref": remote_ref,
            "remote_sha": base,
        }
    ]
    if extra_updates:
        update.extend(extra_updates)
    return {
        "schema": 1,
        "event_type": "receipt_shadow",
        "timestamp": timestamp,
        "gate": "task check",
        "worktree": "/tmp/fixture-worktree",
        "payload": {
            "push_id": push_id,
            "update": update,
            "candidate": candidate,
            "base": base,
            "diff_class": diff_class,
            "decision": decision,
            "reasons": reasons or [],
            "requirements": {"schema": 1, "requirements": [{"gate": "task check", "argv": ["task", "check"]}]},
            "real_gate": {"gate": "task check", "ran": real_gate_ran, "exit_code": real_gate_exit_code},
            "classification": classification,
        },
    }


def write_log(path: Path, events: list[dict]) -> None:
    with open(path, "w", encoding="utf-8") as f:
        for e in events:
            f.write(json.dumps(e) + "\n")


class SingleMainRefUpdateTest(unittest.TestCase):
    def test_single_landed_main_push_returns_update(self) -> None:
        event = make_event(
            push_id="p1", timestamp="2026-09-01T00:00:00Z", local_sha="a" * 40,
            candidate="tree-a", decision="reuse", classification="match",
        )
        self.assertIsNotNone(MODULE.single_main_ref_update(event))

    def test_multi_ref_push_returns_none(self) -> None:
        event = make_event(
            push_id="p2", timestamp="2026-09-01T00:00:00Z", local_sha="a" * 40,
            candidate="tree-a", decision="invalid", classification="expected_miss",
            reasons=["multi_ref"],
            extra_updates=[
                {"local_ref": "refs/heads/feature/x", "local_sha": "b" * 40, "remote_ref": "refs/heads/feature/x", "remote_sha": "c" * 40}
            ],
        )
        self.assertIsNone(MODULE.single_main_ref_update(event))

    def test_feature_branch_push_returns_none(self) -> None:
        event = make_event(
            push_id="p3", timestamp="2026-09-01T00:00:00Z", local_sha="a" * 40,
            candidate="tree-a", decision="invalid", classification="expected_miss",
            reasons=["missing_gate"], remote_ref="refs/heads/feature/x",
        )
        self.assertIsNone(MODULE.single_main_ref_update(event))

    def test_single_push_with_unrelated_deletion_still_counts(self) -> None:
        event = make_event(
            push_id="p4", timestamp="2026-09-01T00:00:00Z", local_sha="a" * 40,
            candidate="tree-a", decision="reuse", classification="match",
            extra_updates=[
                {"local_ref": "(delete)", "local_sha": ZERO_SHA, "remote_ref": "refs/heads/feature/gone", "remote_sha": "c" * 40}
            ],
        )
        self.assertIsNotNone(MODULE.single_main_ref_update(event))

    def test_master_ref_counts_as_main_ref(self) -> None:
        event = make_event(
            push_id="p5", timestamp="2026-09-01T00:00:00Z", local_sha="a" * 40,
            candidate="tree-a", decision="reuse", classification="match",
            remote_ref="refs/heads/master",
        )
        self.assertIsNotNone(MODULE.single_main_ref_update(event))


class AncestryFilteringTest(unittest.TestCase):
    def setUp(self) -> None:
        self.fixture = FixtureRepo()
        self.addCleanup(self.fixture.close)

    def test_landed_commit_is_ancestor(self) -> None:
        sha, _ = self.fixture.commit_and_land("landed")
        self.assertTrue(MODULE.git_is_ancestor(sha, str(self.fixture.repo), "origin/main"))

    def test_abandoned_commit_is_not_ancestor(self) -> None:
        sha, _ = self.fixture.commit_abandoned("abandoned")
        self.assertFalse(MODULE.git_is_ancestor(sha, str(self.fixture.repo), "origin/main"))

    def test_master_landed_commit_is_eligible_end_to_end(self) -> None:
        # Regression for a bug where eligibility hardcoded origin/main even
        # for a push whose remote_ref was refs/heads/master: a master push
        # could never accumulate the 10 eligible events the docstring
        # promises. Land onto origin/master only (origin/main untouched) and
        # confirm is_eligible still recognizes it via the matched ref.
        sha, _ = self.fixture.commit_and_land("landed-master", remote="origin/master")
        event = make_event(
            push_id="p-master", timestamp="2026-09-01T00:00:00Z", local_sha=sha,
            candidate="tree-master", decision="reuse", classification="match",
            remote_ref="refs/heads/master",
        )
        self.assertTrue(MODULE.is_eligible(event, str(self.fixture.repo), MODULE.git_is_ancestor))


class EligibilityAndSelectionTest(unittest.TestCase):
    """End-to-end fixture: a full synthetic event log against a real fixture
    repo, covering every exclusion the acceptance criteria names plus
    deterministic first-10 selection."""

    def setUp(self) -> None:
        self.fixture = FixtureRepo()
        self.addCleanup(self.fixture.close)

    def _clean_events(self, n: int, start_index: int = 0) -> list[dict]:
        events = []
        for i in range(n):
            idx = start_index + i
            sha, tree = self.fixture.commit_and_land(f"landed-{idx}")
            events.append(
                make_event(
                    push_id=f"push-{idx}",
                    timestamp=f"2026-09-01T00:{idx:02d}:00Z",
                    local_sha=sha,
                    candidate=tree,
                    decision="reuse",
                    classification="match",
                )
            )
        return events

    def test_deterministic_first_ten_selection_with_extras(self) -> None:
        clean = self._clean_events(12)
        # Duplicate candidate tree (same tree seen twice) must dedupe.
        dup = make_event(
            push_id="dup-1", timestamp="2026-09-01T00:00:30Z",
            local_sha=clean[0]["payload"]["update"][0]["local_sha"],
            candidate=clean[0]["payload"]["candidate"],
            decision="reuse", classification="match",
        )
        # beads_only diff_class must be excluded.
        beads_sha, beads_tree = self.fixture.commit_and_land("beads-only")
        beads_event = make_event(
            push_id="beads-1", timestamp="2026-09-01T00:00:05Z",
            local_sha=beads_sha, candidate=beads_tree,
            decision="reuse", classification="match", diff_class="beads_only",
        )
        # Abandoned (non-ancestor) push must be excluded.
        abandoned_sha, abandoned_tree = self.fixture.commit_abandoned("abandoned-1")
        abandoned_event = make_event(
            push_id="abandoned-1", timestamp="2026-09-01T00:00:06Z",
            local_sha=abandoned_sha, candidate=abandoned_tree,
            decision="reuse", classification="match",
        )
        # Multi-ref push must be excluded.
        multi_sha, multi_tree = self.fixture.commit_and_land("multi-1")
        multi_event = make_event(
            push_id="multi-1", timestamp="2026-09-01T00:00:07Z",
            local_sha=multi_sha, candidate=multi_tree,
            decision="invalid", classification="expected_miss", reasons=["multi_ref"],
            extra_updates=[
                {"local_ref": "refs/heads/feature/y", "local_sha": "d" * 40, "remote_ref": "refs/heads/feature/y", "remote_sha": "e" * 40}
            ],
        )

        all_events = clean + [dup, beads_event, abandoned_event, multi_event]
        eligible = MODULE.eligible_events(all_events, str(self.fixture.repo), MODULE.git_is_ancestor)
        # 12 clean + 1 duplicate = 13 eligible before dedup; beads/abandoned/multi excluded.
        self.assertEqual(len(eligible), 13)

        selected = MODULE.select_first_n_distinct(eligible, 10)
        self.assertEqual(len(selected), 10)
        # Deterministic: first 10 distinct candidate trees in timestamp order.
        expected_trees = [e["payload"]["candidate"] for e in clean[:10]]
        self.assertEqual([e["payload"]["candidate"] for e in selected], expected_trees)

    def test_fewer_than_limit_eligible_is_insufficient_not_failed(self) -> None:
        clean = self._clean_events(3)
        eligible = MODULE.eligible_events(clean, str(self.fixture.repo), MODULE.git_is_ancestor)
        selected = MODULE.select_first_n_distinct(eligible, 10)
        records = [MODULE.project_record(e) for e in selected]
        result = MODULE.evaluate_threshold(records, {}, 10)
        self.assertTrue(result["insufficient_evidence"])
        self.assertFalse(result["passed"])


class ThresholdRuleTest(unittest.TestCase):
    def setUp(self) -> None:
        self.fixture = FixtureRepo()
        self.addCleanup(self.fixture.close)

    def _ten_clean_records(self) -> list[dict]:
        events = []
        for i in range(10):
            sha, tree = self.fixture.commit_and_land(f"clean-{i}")
            events.append(
                make_event(
                    push_id=f"clean-{i}", timestamp=f"2026-09-02T00:{i:02d}:00Z",
                    local_sha=sha, candidate=tree,
                    decision="reuse", classification="match",
                )
            )
        eligible = MODULE.eligible_events(events, str(self.fixture.repo), MODULE.git_is_ancestor)
        selected = MODULE.select_first_n_distinct(eligible, 10)
        return [MODULE.project_record(e) for e in selected]

    def test_all_clean_passes(self) -> None:
        records = self._ten_clean_records()
        result = MODULE.evaluate_threshold(records, {}, 10)
        self.assertTrue(result["passed"])
        self.assertFalse(result["false_accepts"])
        self.assertFalse(result["unexpected_misses_unexplained"])

    def test_single_false_accept_fails(self) -> None:
        records = self._ten_clean_records()
        records[3]["classification"] = "false_accept"
        result = MODULE.evaluate_threshold(records, {}, 10)
        self.assertFalse(result["passed"])
        self.assertEqual(len(result["false_accepts"]), 1)

    def test_unexplained_unexpected_miss_fails(self) -> None:
        records = self._ten_clean_records()
        records[5]["classification"] = "unexpected_miss"
        records[5]["reasons"] = []
        result = MODULE.evaluate_threshold(records, {}, 10)
        self.assertFalse(result["passed"])
        self.assertEqual(len(result["unexpected_misses_unexplained"]), 1)

    def test_unexpected_miss_with_linked_issue_is_explained_and_passes(self) -> None:
        records = self._ten_clean_records()
        records[5]["classification"] = "unexpected_miss"
        records[5]["reasons"] = []
        candidate = records[5]["candidate"]
        annotations = {candidate: "str-fake-issue-1"}
        result = MODULE.evaluate_threshold(records, annotations, 10)
        self.assertTrue(result["passed"])
        self.assertEqual(len(result["unexpected_misses_explained"]), 1)
        self.assertEqual(
            result["unexpected_misses_explained"][0]["linked_issue"], "str-fake-issue-1"
        )

    def test_expected_miss_without_reasons_is_flagged(self) -> None:
        records = self._ten_clean_records()
        records[2]["classification"] = "expected_miss"
        records[2]["reasons"] = []
        result = MODULE.evaluate_threshold(records, {}, 10)
        self.assertFalse(result["passed"])
        self.assertEqual(len(result["expected_misses_missing_reasons"]), 1)

    def test_expected_miss_with_reasons_does_not_block(self) -> None:
        records = self._ten_clean_records()
        records[2]["classification"] = "expected_miss"
        records[2]["reasons"] = ["argv"]
        result = MODULE.evaluate_threshold(records, {}, 10)
        self.assertTrue(result["passed"])


class NoteAppendTest(unittest.TestCase):
    """The durable note-append path, with the real `bd note` invocation
    stubbed -- these tests must never hit the real tracker."""

    def test_append_note_success_is_reported(self) -> None:
        calls = []

        def fake_run(args, **kwargs):
            calls.append(args)
            return subprocess.CompletedProcess(args, 0, stdout="", stderr="")

        ok, err = MODULE.append_note("str-fake-9", "milestone met", run=fake_run)
        self.assertTrue(ok)
        self.assertEqual(err, "")
        self.assertEqual(calls, [["bd", "note", "str-fake-9", "milestone met"]])

    def test_append_note_failure_is_reported(self) -> None:
        def fake_run(args, **kwargs):
            return subprocess.CompletedProcess(args, 1, stdout="", stderr="boom")

        ok, err = MODULE.append_note("str-fake-9", "milestone met", run=fake_run)
        self.assertFalse(ok)
        self.assertEqual(err, "boom")


class FetchInjectionTest(unittest.TestCase):
    def test_fetch_origin_uses_injected_runner(self) -> None:
        calls = []

        def fake_run(args, **kwargs):
            calls.append(args)
            return subprocess.CompletedProcess(args, 0)

        MODULE.fetch_origin("/some/repo", run=fake_run)
        self.assertEqual(calls, [["git", "fetch", "origin"]])


class EndToEndCliTest(unittest.TestCase):
    """Drives the actual CLI entrypoint against a fixture log and fixture
    repo, with --skip-fetch semantics (no --fetch flag == no network) and a
    stubbed note-append path."""

    def setUp(self) -> None:
        self.fixture = FixtureRepo()
        self.addCleanup(self.fixture.close)

    def _write_ten_clean(self, log_path: Path) -> None:
        events = []
        for i in range(10):
            sha, tree = self.fixture.commit_and_land(f"cli-clean-{i}")
            events.append(
                make_event(
                    push_id=f"cli-clean-{i}", timestamp=f"2026-09-03T00:{i:02d}:00Z",
                    local_sha=sha, candidate=tree,
                    decision="reuse", classification="match",
                )
            )
        write_log(log_path, events)

    def test_cli_passes_on_ten_clean_events_and_exits_zero(self) -> None:
        log_path = self.fixture.root / "events.jsonl"
        self._write_ten_clean(log_path)
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "--event-log-path", str(log_path), "--repo", str(self.fixture.repo)],
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("MET", result.stdout)

    def test_cli_reports_insufficient_evidence_and_exits_nonzero(self) -> None:
        log_path = self.fixture.root / "events.jsonl"
        events = []
        for i in range(3):
            sha, tree = self.fixture.commit_and_land(f"cli-few-{i}")
            events.append(
                make_event(
                    push_id=f"cli-few-{i}", timestamp=f"2026-09-03T00:{i:02d}:00Z",
                    local_sha=sha, candidate=tree,
                    decision="reuse", classification="match",
                )
            )
        write_log(log_path, events)
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "--event-log-path", str(log_path), "--repo", str(self.fixture.repo)],
            capture_output=True, text=True, check=False,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("insufficient", result.stdout.lower())

    def test_cli_json_output_matches_evaluate_threshold_shape(self) -> None:
        log_path = self.fixture.root / "events.jsonl"
        self._write_ten_clean(log_path)
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "--event-log-path", str(log_path), "--repo", str(self.fixture.repo), "--json"],
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        parsed = json.loads(result.stdout)
        self.assertTrue(parsed["passed"])
        self.assertEqual(parsed["selected_count"], 10)

    def test_cli_does_not_fetch_by_default(self) -> None:
        # No remote named "origin" is configured for real (only a local
        # refs/remotes/origin/main ref) -- a real `git fetch origin` here
        # would fail/hang, so absence of --fetch must mean no such attempt.
        log_path = self.fixture.root / "events.jsonl"
        self._write_ten_clean(log_path)
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "--event-log-path", str(log_path), "--repo", str(self.fixture.repo)],
            capture_output=True, text=True, check=False, timeout=15,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_cli_apply_note_path_with_stubbed_bd(self) -> None:
        # Stub `bd` as a fake executable on PATH that just echoes success,
        # proving the append-note wiring runs end-to-end without touching
        # the real tracker.
        log_path = self.fixture.root / "events.jsonl"
        self._write_ten_clean(log_path)
        bin_dir = self.fixture.root / "fakebin"
        bin_dir.mkdir()
        fake_bd = bin_dir / "bd"
        fake_bd.write_text("#!/bin/sh\necho \"fake bd called with: $@\"\nexit 0\n")
        fake_bd.chmod(0o755)

        import os as _os
        env = dict(_os.environ)
        env["PATH"] = f"{bin_dir}:{env.get('PATH', '')}"

        result = subprocess.run(
            [
                sys.executable, str(SCRIPT),
                "--event-log-path", str(log_path),
                "--repo", str(self.fixture.repo),
                "--apply-note", "--note-issue", "str-fake-9",
            ],
            capture_output=True, text=True, check=False, env=env,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("Appended note to str-fake-9", result.stdout)

    def test_cli_annotations_file_explains_unexpected_miss(self) -> None:
        log_path = self.fixture.root / "events.jsonl"
        events = []
        for i in range(9):
            sha, tree = self.fixture.commit_and_land(f"cli-ann-{i}")
            events.append(
                make_event(
                    push_id=f"cli-ann-{i}", timestamp=f"2026-09-04T00:{i:02d}:00Z",
                    local_sha=sha, candidate=tree,
                    decision="reuse", classification="match",
                )
            )
        miss_sha, miss_tree = self.fixture.commit_and_land("cli-ann-miss")
        events.append(
            make_event(
                push_id="cli-ann-miss", timestamp="2026-09-04T00:09:00Z",
                local_sha=miss_sha, candidate=miss_tree,
                decision="invalid", classification="unexpected_miss", reasons=[],
            )
        )
        write_log(log_path, events)

        annotations_path = self.fixture.root / "annotations.json"
        annotations_path.write_text(json.dumps({miss_tree: "str-tracked-1"}))

        result = subprocess.run(
            [
                sys.executable, str(SCRIPT),
                "--event-log-path", str(log_path),
                "--repo", str(self.fixture.repo),
                "--annotations", str(annotations_path),
                "--json",
            ],
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        parsed = json.loads(result.stdout)
        self.assertTrue(parsed["passed"])
        self.assertEqual(len(parsed["unexpected_misses_explained"]), 1)
        self.assertEqual(
            parsed["unexpected_misses_explained"][0]["linked_issue"], "str-tracked-1"
        )


if __name__ == "__main__":
    unittest.main()
