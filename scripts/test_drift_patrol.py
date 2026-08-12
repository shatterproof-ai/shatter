"""Tests for scripts/drift-patrol.py (str-u394l.1)."""

from __future__ import annotations

import importlib.util
import sys
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path
from unittest import mock

MODULE_PATH = Path(__file__).with_name("drift-patrol.py")
SPEC = importlib.util.spec_from_file_location("drift_patrol", MODULE_PATH)
assert SPEC is not None
assert SPEC.loader is not None
drift_patrol = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = drift_patrol
SPEC.loader.exec_module(drift_patrol)

NOW = datetime(2026, 7, 28, tzinfo=timezone.utc)


def _issue(ident: str, status: str, *, days_stale: int = 0, **extra: object) -> dict:
    updated = NOW - timedelta(days=days_stale)
    issue = {
        "id": ident,
        "title": f"title for {ident}",
        "status": status,
        "issue_type": extra.pop("issue_type", "task"),
        "updated_at": updated.strftime("%Y-%m-%dT%H:%M:%SZ"),
    }
    issue.update(extra)
    return issue


def _tracker(*issues: dict) -> object:
    return drift_patrol.Tracker(list(issues), "test fixture")


# ---------------------------------------------------------------------------
# Tracker hygiene
# ---------------------------------------------------------------------------


class TrackerHygieneTest(unittest.TestCase):
    def test_clean_tracker_passes(self) -> None:
        tracker = _tracker(
            _issue("str-a", "in_progress", days_stale=3),
            _issue("str-b", "open", days_stale=400),  # only in_progress is patrolled
            _issue("str-c", "closed", days_stale=400),
        )
        result = drift_patrol.check_tracker_hygiene(tracker=tracker, now=NOW)
        self.assertEqual(result.status, drift_patrol.PASS)
        self.assertFalse(result.failed)

    def test_stale_in_progress_fails_and_names_the_issue(self) -> None:
        tracker = _tracker(_issue("str-stale", "in_progress", days_stale=30))
        result = drift_patrol.check_tracker_hygiene(tracker=tracker, now=NOW)
        self.assertEqual(result.status, drift_patrol.FAIL)
        self.assertTrue(any("str-stale" in line for line in result.details))
        self.assertIn("1 stale", result.summary)

    def test_threshold_is_exclusive_at_the_boundary(self) -> None:
        exactly = _tracker(_issue("str-edge", "in_progress", days_stale=14))
        self.assertEqual(
            drift_patrol.check_tracker_hygiene(tracker=exactly, now=NOW).status,
            drift_patrol.PASS,
        )
        over = _tracker(_issue("str-edge", "in_progress", days_stale=15))
        self.assertEqual(
            drift_patrol.check_tracker_hygiene(tracker=over, now=NOW).status,
            drift_patrol.FAIL,
        )

    def test_custom_stale_days_is_honored(self) -> None:
        tracker = _tracker(_issue("str-x", "in_progress", days_stale=20))
        self.assertEqual(
            drift_patrol.check_tracker_hygiene(
                tracker=tracker, now=NOW, stale_days=30
            ).status,
            drift_patrol.PASS,
        )

    def test_orphaned_child_under_closed_epic_fails(self) -> None:
        tracker = _tracker(
            _issue("str-epic", "closed", issue_type="epic"),
            _issue("str-epic.1", "in_progress"),
        )
        result = drift_patrol.check_tracker_hygiene(tracker=tracker, now=NOW)
        self.assertEqual(result.status, drift_patrol.FAIL)
        self.assertIn("1 orphaned", result.summary)
        self.assertTrue(any("str-epic.1" in line for line in result.details))

    def test_closed_child_under_closed_epic_is_not_an_orphan(self) -> None:
        tracker = _tracker(
            _issue("str-epic", "closed", issue_type="epic"),
            _issue("str-epic.1", "closed"),
        )
        result = drift_patrol.check_tracker_hygiene(tracker=tracker, now=NOW)
        self.assertEqual(result.status, drift_patrol.PASS)

    def test_child_under_open_epic_is_not_an_orphan(self) -> None:
        tracker = _tracker(
            _issue("str-epic", "open", issue_type="epic"),
            _issue("str-epic.1", "open"),
        )
        result = drift_patrol.check_tracker_hygiene(tracker=tracker, now=NOW)
        self.assertEqual(result.status, drift_patrol.PASS)

    def test_explicit_parent_field_beats_dotted_id(self) -> None:
        tracker = _tracker(
            _issue("str-parent", "closed"),
            _issue("str-child", "open", parent="str-parent"),
        )
        result = drift_patrol.check_tracker_hygiene(tracker=tracker, now=NOW)
        self.assertEqual(result.status, drift_patrol.FAIL)

    def test_missing_tracker_data_skips_rather_than_fails(self) -> None:
        result = drift_patrol.check_tracker_hygiene(
            tracker=None, tracker_error="bd unavailable", now=NOW
        )
        self.assertEqual(result.status, drift_patrol.SKIP)
        self.assertFalse(result.failed)

    def test_long_finding_lists_are_truncated(self) -> None:
        many = [
            _issue(f"str-{n}", "in_progress", days_stale=100)
            for n in range(drift_patrol.MAX_ITEMS_REPORTED + 5)
        ]
        result = drift_patrol.check_tracker_hygiene(tracker=_tracker(*many), now=NOW)
        self.assertEqual(result.status, drift_patrol.FAIL)
        self.assertTrue(any("and 5 more" in line for line in result.details))


# ---------------------------------------------------------------------------
# Placeholder checks for work tracked elsewhere
# ---------------------------------------------------------------------------


class PendingCheckTest(unittest.TestCase):
    def _pending(self, tracker: object | None, **kwargs: object) -> object:
        return drift_patrol.pending_check(
            check_id="demo",
            title="Demo check",
            tracking_issue="str-dep",
            what="The demo gate",
            remediation="Land str-dep.",
            tracker=tracker,
            strict_pending=bool(kwargs.pop("strict_pending", False)),
            **kwargs,
        )

    def test_open_dependency_reports_pending_not_failure(self) -> None:
        result = self._pending(_tracker(_issue("str-dep", "open")))
        self.assertEqual(result.status, drift_patrol.PENDING)
        self.assertFalse(result.failed)
        self.assertEqual(result.tracking_issue, "str-dep")

    def test_strict_pending_promotes_to_failure(self) -> None:
        result = self._pending(_tracker(_issue("str-dep", "open")), strict_pending=True)
        self.assertEqual(result.status, drift_patrol.FAIL)

    def test_closed_dependency_with_placeholder_still_present_fails(self) -> None:
        result = self._pending(_tracker(_issue("str-dep", "closed")))
        self.assertEqual(result.status, drift_patrol.FAIL)
        self.assertIn("closed", result.summary)

    def test_closed_dependency_whose_work_landed_is_not_a_failure(self) -> None:
        result = self._pending(_tracker(_issue("str-dep", "closed")), landed=True)
        self.assertEqual(result.status, drift_patrol.PENDING)

    def test_unknown_tracker_state_degrades_to_pending(self) -> None:
        result = self._pending(None)
        self.assertEqual(result.status, drift_patrol.PENDING)
        self.assertIn("unknown", result.summary)


# ---------------------------------------------------------------------------
# docs/stories
# ---------------------------------------------------------------------------


class DocsStoriesTest(unittest.TestCase):
    def test_absent_stories_dir_is_pending_on_the_stories_issue(self) -> None:
        # docs/stories does not exist in this repo yet; if it ever does, this
        # test is the signal to replace the placeholder assertion.
        if (drift_patrol.REPO_ROOT / "docs" / "stories").is_dir():
            self.skipTest("docs/stories now exists — check the real path instead")
        result = drift_patrol.check_docs_stories(
            tracker=_tracker(_issue("str-u394l.3", "open"))
        )
        self.assertEqual(result.status, drift_patrol.PENDING)
        self.assertEqual(result.tracking_issue, "str-u394l.3")


# ---------------------------------------------------------------------------
# Subprocess helper
# ---------------------------------------------------------------------------


class RunCommandTest(unittest.TestCase):
    """run_command() must degrade, never raise (str-ny1jh).

    Every check funnels through this helper, so an exception escaping it
    takes down the whole patrol instead of turning one check red.
    """

    def test_missing_command_reports_127(self) -> None:
        with mock.patch.object(
            drift_patrol.subprocess, "run", side_effect=FileNotFoundError("no such file")
        ):
            code, output = drift_patrol.run_command(["definitely-not-a-real-command"])
        self.assertEqual(code, 127)
        self.assertIn("command not found", output)

    def test_timeout_reports_124(self) -> None:
        with mock.patch.object(
            drift_patrol.subprocess,
            "run",
            side_effect=drift_patrol.subprocess.TimeoutExpired(cmd="sleep", timeout=5),
        ):
            code, output = drift_patrol.run_command(["sleep", "99"], timeout=5)
        self.assertEqual(code, 124)
        self.assertIn("timed out after 5s", output)

    def test_permission_error_degrades_instead_of_raising(self) -> None:
        # A checker script that lost its +x bit: PermissionError is an OSError
        # but not a FileNotFoundError, so it used to propagate uncaught.
        with mock.patch.object(
            drift_patrol.subprocess,
            "run",
            side_effect=PermissionError(13, "Permission denied"),
        ):
            code, output = drift_patrol.run_command(["scripts/not-executable.py"])
        self.assertEqual(code, 1)
        self.assertIn("could not run", output)
        self.assertIn("Permission denied", output)

    def test_other_oserror_degrades_instead_of_raising(self) -> None:
        with mock.patch.object(
            drift_patrol.subprocess,
            "run",
            side_effect=OSError(12, "Cannot allocate memory"),
        ):
            code, output = drift_patrol.run_command(["some-checker"])
        self.assertEqual(code, 1)
        self.assertIn("Cannot allocate memory", output)

    def test_oserror_surfaces_as_a_failed_check_not_a_crash(self) -> None:
        # End-to-end through _subprocess_check: the patrol keeps running and
        # reports FAIL for the one check that could not be executed.
        with mock.patch.object(
            drift_patrol.subprocess,
            "run",
            side_effect=PermissionError(13, "Permission denied"),
        ):
            result = drift_patrol.check_protocol_registry()
        self.assertEqual(result.status, drift_patrol.FAIL)
        self.assertIn("exit 1", result.summary)


# ---------------------------------------------------------------------------
# Helpers and reporting
# ---------------------------------------------------------------------------


class LoadTrackerTest(unittest.TestCase):
    """Covers load_tracker()'s bd/jsonl dual-path and failure degradation.

    Flagged by independent review as safety-critical but previously
    untested: every other test builds a Tracker(...) directly, bypassing
    load_tracker() entirely, so a regression here would only surface as a
    live CI failure rather than a unit-test failure.
    """

    def _write_export(self, tmp: Path, lines: list[str]) -> None:
        beads_dir = tmp / ".beads"
        beads_dir.mkdir(parents=True, exist_ok=True)
        (beads_dir / "issues.jsonl").write_text("\n".join(lines) + "\n")

    def test_bd_available_and_parses_uses_bd_source(self) -> None:
        payload = '[{"id": "str-a", "status": "open"}]'
        with (
            mock.patch.object(drift_patrol, "_which", return_value=True),
            mock.patch.object(
                drift_patrol, "run_command", return_value=(0, payload)
            ) as run_command,
        ):
            tracker, error = drift_patrol.load_tracker()
        self.assertIsNone(error)
        assert tracker is not None
        self.assertEqual(tracker.source, "bd list --all")
        self.assertEqual(tracker.issues, [{"id": "str-a", "status": "open"}])
        run_command.assert_called_once()
        self.assertEqual(run_command.call_args.args[0][0], "bd")

    def test_bd_present_but_errors_falls_back_to_jsonl(self) -> None:
        with tempfile.TemporaryDirectory() as raw_tmp:
            tmp = Path(raw_tmp)
            self._write_export(tmp, ['{"id": "str-b", "status": "closed"}'])
            with (
                mock.patch.object(drift_patrol, "_which", return_value=True),
                mock.patch.object(
                    drift_patrol, "run_command", return_value=(1, "bd: connection refused")
                ),
                mock.patch.object(drift_patrol, "REPO_ROOT", tmp),
            ):
                tracker, error = drift_patrol.load_tracker()
        self.assertIsNone(error)
        assert tracker is not None
        self.assertEqual(tracker.source, ".beads/issues.jsonl (committed export)")
        self.assertEqual(tracker.issues, [{"id": "str-b", "status": "closed"}])

    def test_bd_returns_empty_list_falls_back_to_jsonl(self) -> None:
        # bd exits 0 but with an empty/parseable-but-falsy payload -- same
        # fallback path as a bd error, per load_tracker()'s `and parsed` guard.
        with tempfile.TemporaryDirectory() as raw_tmp:
            tmp = Path(raw_tmp)
            self._write_export(tmp, ['{"id": "str-c", "status": "open"}'])
            with (
                mock.patch.object(drift_patrol, "_which", return_value=True),
                mock.patch.object(drift_patrol, "run_command", return_value=(0, "[]")),
                mock.patch.object(drift_patrol, "REPO_ROOT", tmp),
            ):
                tracker, error = drift_patrol.load_tracker()
        assert tracker is not None
        self.assertEqual(tracker.source, ".beads/issues.jsonl (committed export)")

    def test_no_bd_reads_jsonl_and_skips_malformed_lines(self) -> None:
        with tempfile.TemporaryDirectory() as raw_tmp:
            tmp = Path(raw_tmp)
            self._write_export(
                tmp,
                [
                    '{"id": "str-d", "status": "open"}',
                    "not json at all",
                    "",
                    '{"id": "str-e", "status": "in_progress"}',
                ],
            )
            with (
                mock.patch.object(drift_patrol, "_which", return_value=False),
                mock.patch.object(drift_patrol, "REPO_ROOT", tmp),
            ):
                tracker, error = drift_patrol.load_tracker()
        self.assertIsNone(error)
        assert tracker is not None
        self.assertEqual([i["id"] for i in tracker.issues], ["str-d", "str-e"])

    def test_no_bd_and_no_jsonl_returns_reason(self) -> None:
        with tempfile.TemporaryDirectory() as raw_tmp:
            tmp = Path(raw_tmp)
            with (
                mock.patch.object(drift_patrol, "_which", return_value=False),
                mock.patch.object(drift_patrol, "REPO_ROOT", tmp),
            ):
                tracker, error = drift_patrol.load_tracker()
        self.assertIsNone(tracker)
        assert error is not None
        self.assertIn("absent", error)

    def test_jsonl_with_only_malformed_lines_returns_reason(self) -> None:
        with tempfile.TemporaryDirectory() as raw_tmp:
            tmp = Path(raw_tmp)
            self._write_export(tmp, ["not json", "{broken"])
            with (
                mock.patch.object(drift_patrol, "_which", return_value=False),
                mock.patch.object(drift_patrol, "REPO_ROOT", tmp),
            ):
                tracker, error = drift_patrol.load_tracker()
        self.assertIsNone(tracker)
        assert error is not None
        self.assertIn("no issue records", error)

    def test_jsonl_skips_non_issue_record_types(self) -> None:
        with tempfile.TemporaryDirectory() as raw_tmp:
            tmp = Path(raw_tmp)
            self._write_export(
                tmp,
                [
                    '{"id": "str-f", "status": "open", "_type": "issue"}',
                    '{"id": "dep-1", "_type": "dependency"}',
                ],
            )
            with (
                mock.patch.object(drift_patrol, "_which", return_value=False),
                mock.patch.object(drift_patrol, "REPO_ROOT", tmp),
            ):
                tracker, error = drift_patrol.load_tracker()
        self.assertIsNone(error)
        assert tracker is not None
        self.assertEqual([i["id"] for i in tracker.issues], ["str-f"])


class HelpersTest(unittest.TestCase):
    def test_parent_id_from_dotted_id(self) -> None:
        self.assertEqual(drift_patrol.parent_id({"id": "str-abc.1"}), "str-abc")
        self.assertEqual(drift_patrol.parent_id({"id": "str-abc.1.2"}), "str-abc.1")
        self.assertIsNone(drift_patrol.parent_id({"id": "str-abc"}))

    def test_parse_ts_handles_z_suffix_and_bad_input(self) -> None:
        self.assertEqual(
            drift_patrol.parse_ts("2026-07-28T00:00:00Z"),
            datetime(2026, 7, 28, tzinfo=timezone.utc),
        )
        self.assertIsNone(drift_patrol.parse_ts(None))
        self.assertIsNone(drift_patrol.parse_ts("not a date"))

    def test_tail_drops_blank_lines_and_bounds_length(self) -> None:
        text = "\n".join(["a", "", "b", "c"])
        self.assertEqual(drift_patrol.tail(text, lines=2), ["b", "c"])


class ReportTest(unittest.TestCase):
    def _results(self) -> list[object]:
        return [
            drift_patrol.Result("ok", "Fine", drift_patrol.PASS, "all good"),
            drift_patrol.Result(
                "bad",
                "Broken",
                drift_patrol.FAIL,
                "found drift",
                details=["offending-item"],
                tracking_issue="str-dep",
                remediation="Do the thing.",
            ),
        ]

    def test_report_includes_issue_and_remediation(self) -> None:
        report = drift_patrol.render_report(self._results(), timestamp="2026-07-28")
        self.assertIn("`str-dep`", report)
        self.assertIn("Do the thing.", report)
        self.assertIn("offending-item", report)
        self.assertIn("File or update a Beads issue", report)

    def test_clean_report_says_nothing_to_file(self) -> None:
        clean = [drift_patrol.Result("ok", "Fine", drift_patrol.PASS, "all good")]
        report = drift_patrol.render_report(clean, timestamp="2026-07-28")
        self.assertIn("Nothing to file.", report)

    def test_pipes_in_summaries_do_not_break_the_table(self) -> None:
        results = [drift_patrol.Result("x", "T", drift_patrol.PASS, "a | b")]
        report = drift_patrol.render_report(results, timestamp="2026-07-28")
        self.assertIn(r"a \| b", report)


class CliTest(unittest.TestCase):
    def test_check_ids_cover_every_registered_check(self) -> None:
        self.assertEqual(len(drift_patrol.CHECK_IDS), len(drift_patrol.CHECKS))
        self.assertEqual(len(set(drift_patrol.CHECK_IDS)), len(drift_patrol.CHECKS))

    def test_every_check_accepts_the_shared_context(self) -> None:
        context = {
            "tracker": None,
            "tracker_error": "test",
            "stale_days": 14,
            "warn_within_days": 14,
            "require_conformance": False,
            "strict_pending": False,
            "now": NOW,
        }
        # Only the pure checks are exercised here; the subprocess-backed ones
        # are covered by the patrol's own end-to-end run in CI.
        for check_id in ("cli-surface-drift", "docs-stories", "tracker-hygiene"):
            fn = dict(drift_patrol.CHECKS)[check_id]
            result = fn(**context)
            self.assertEqual(result.check_id, check_id)

    def test_skipping_every_check_is_a_usage_error(self) -> None:
        argv: list[str] = []
        for check_id in drift_patrol.CHECK_IDS:
            argv += ["--skip", check_id]
        self.assertEqual(drift_patrol.main(argv), 2)

    def test_invalid_today_is_a_usage_error(self) -> None:
        self.assertEqual(
            drift_patrol.main(["--only", "tracker-hygiene", "--today", "nope"]), 2
        )


if __name__ == "__main__":
    unittest.main()
