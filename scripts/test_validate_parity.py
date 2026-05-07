"""Tests for scripts/validate-parity.py."""

from __future__ import annotations

import datetime as _dt
import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("validate-parity.py")
SPEC = importlib.util.spec_from_file_location("validate_parity", MODULE_PATH)
assert SPEC is not None
assert SPEC.loader is not None
validate_parity = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = validate_parity
SPEC.loader.exec_module(validate_parity)


# ---------------------------------------------------------------------------
# Fixtures
# ---------------------------------------------------------------------------


def _full_entry(**overrides: object) -> dict:
    """Build a fully-valid divergence entry; pass None to drop a field."""
    base: dict = {
        "id": "ts-missing-teardown",
        "description": "TS does not advertise teardown.",
        "affected_frontends": ["typescript"],
        "affected_commands": ["teardown"],
        "status": "tracked",
        "owner": "Test Owner",
        "tracking_issue": "str-1hlk.13",
        "resolution_condition": "teardown is in SUPPORTED_CAPABILITIES",
        "resolution": "Implement TS teardown.",
    }
    for k, v in overrides.items():
        if v is None:
            base.pop(k, None)
        else:
            base[k] = v
    return base


def _md_with_ids(*ids: str) -> str:
    blocks = "\n\n".join(f"### `{i}`\n\nstub\n" for i in ids)
    return (
        "# Frontend Parity Contract\n"
        "\n"
        "## Allowed Divergences\n"
        "\n"
        "intro paragraph\n"
        "\n"
        f"{blocks}\n"
        "\n"
        "## Adding a New Divergence\n"
        "\n"
        "bla\n"
    )


def _run_validate_md(md_text: str, allowed: list[dict], today: _dt.date):
    """Invoke validate_divergence_metadata against an in-memory PARITY.md."""
    with tempfile.TemporaryDirectory() as tmp:
        md = Path(tmp) / "PARITY.md"
        md.write_text(md_text)
        result = validate_parity.Result()
        validate_parity.validate_divergence_metadata(
            allowed,
            validate_parity.parity_md_divergence_ids(md),
            today,
            result,
        )
        return result


# ---------------------------------------------------------------------------
# Existing tests
# ---------------------------------------------------------------------------


class LoadMatrixTest(unittest.TestCase):
    def test_rejects_duplicate_keys(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            matrix = Path(tmp) / "parity-matrix.yaml"
            matrix.write_text(
                """
version: "0.1.0"
commands: {}
commands:
  handshake:
    status: required
    frontends: {}
complex_type_capabilities: {}
""".lstrip()
            )

            with self.assertRaises(SystemExit):
                validate_parity.load_matrix(matrix)


# ---------------------------------------------------------------------------
# Divergence metadata enforcement (str-1hlk.12)
# ---------------------------------------------------------------------------


class DivergenceMetadataTest(unittest.TestCase):
    today = _dt.date(2026, 5, 6)

    def test_happy_path_passes(self) -> None:
        entry = _full_entry()
        result = _run_validate_md(_md_with_ids(entry["id"]), [entry], self.today)
        self.assertTrue(result.ok(), msg=f"unexpected errors: {result.errors}")

    def test_missing_owner_fails(self) -> None:
        entry = _full_entry(owner=None)
        result = _run_validate_md(_md_with_ids(entry["id"]), [entry], self.today)
        self.assertFalse(result.ok())
        self.assertTrue(any("'owner'" in e for e in result.errors))

    def test_missing_tracking_issue_fails(self) -> None:
        entry = _full_entry(tracking_issue=None)
        result = _run_validate_md(_md_with_ids(entry["id"]), [entry], self.today)
        self.assertFalse(result.ok())
        self.assertTrue(any("'tracking_issue'" in e for e in result.errors))

    def test_missing_resolution_condition_fails(self) -> None:
        entry = _full_entry(resolution_condition=None)
        result = _run_validate_md(_md_with_ids(entry["id"]), [entry], self.today)
        self.assertFalse(result.ok())
        self.assertTrue(any("'resolution_condition'" in e for e in result.errors))

    def test_empty_owner_string_fails(self) -> None:
        entry = _full_entry(owner="   ")
        result = _run_validate_md(_md_with_ids(entry["id"]), [entry], self.today)
        self.assertFalse(result.ok())
        self.assertTrue(any("'owner'" in e for e in result.errors))

    def test_unknown_status_fails(self) -> None:
        entry = _full_entry(status="pending")
        result = _run_validate_md(_md_with_ids(entry["id"]), [entry], self.today)
        self.assertFalse(result.ok())
        self.assertTrue(any("unknown status" in e for e in result.errors))

    def test_tracking_issue_none_only_for_accepted(self) -> None:
        entry = _full_entry(status="tracked", tracking_issue="none")
        result = _run_validate_md(_md_with_ids(entry["id"]), [entry], self.today)
        self.assertFalse(result.ok())
        self.assertTrue(
            any("only allowed for status='accepted'" in e for e in result.errors)
        )

    def test_tracking_issue_none_ok_for_accepted(self) -> None:
        entry = _full_entry(status="accepted", tracking_issue="none")
        result = _run_validate_md(_md_with_ids(entry["id"]), [entry], self.today)
        self.assertTrue(result.ok(), msg=f"unexpected errors: {result.errors}")

    def test_resolved_requires_resolved_at(self) -> None:
        entry = _full_entry(status="resolved")
        result = _run_validate_md(_md_with_ids(entry["id"]), [entry], self.today)
        self.assertFalse(result.ok())
        self.assertTrue(any("requires 'resolved_at'" in e for e in result.errors))

    def test_resolved_within_grace_warns(self) -> None:
        resolved_at = (self.today - _dt.timedelta(days=15)).isoformat()
        entry = _full_entry(status="resolved", resolved_at=resolved_at)
        result = _run_validate_md(_md_with_ids(entry["id"]), [entry], self.today)
        self.assertTrue(result.ok(), msg=f"unexpected errors: {result.errors}")
        self.assertTrue(any("schedule removal" in w for w in result.warnings))

    def test_resolved_past_grace_fails(self) -> None:
        resolved_at = (self.today - _dt.timedelta(days=31)).isoformat()
        entry = _full_entry(status="resolved", resolved_at=resolved_at)
        result = _run_validate_md(_md_with_ids(entry["id"]), [entry], self.today)
        self.assertFalse(result.ok())
        self.assertTrue(
            any("grace period of 30 days has expired" in e for e in result.errors)
        )

    def test_resolved_at_invalid_date_fails(self) -> None:
        entry = _full_entry(status="resolved", resolved_at="last tuesday")
        result = _run_validate_md(_md_with_ids(entry["id"]), [entry], self.today)
        self.assertFalse(result.ok())
        self.assertTrue(any("not a valid YYYY-MM-DD" in e for e in result.errors))

    def test_id_in_yaml_missing_from_md_fails(self) -> None:
        entry = _full_entry()
        result = _run_validate_md(_md_with_ids(), [entry], self.today)
        self.assertFalse(result.ok())
        self.assertTrue(
            any("missing from protocol/PARITY.md" in e for e in result.errors)
        )

    def test_id_in_md_missing_from_yaml_fails(self) -> None:
        entry = _full_entry()
        result = _run_validate_md(
            _md_with_ids(entry["id"], "phantom-divergence"), [entry], self.today
        )
        self.assertFalse(result.ok())
        self.assertTrue(
            any(
                "phantom-divergence" in e and "missing from parity-matrix.yaml" in e
                for e in result.errors
            )
        )

    def test_duplicate_id_fails(self) -> None:
        entry = _full_entry()
        result = _run_validate_md(
            _md_with_ids(entry["id"]), [entry, _full_entry()], self.today
        )
        self.assertFalse(result.ok())
        self.assertTrue(any("duplicate id" in e for e in result.errors))

    def test_empty_affected_lists_fail(self) -> None:
        entry = _full_entry(affected_frontends=[])
        result = _run_validate_md(_md_with_ids(entry["id"]), [entry], self.today)
        self.assertFalse(result.ok())
        self.assertTrue(
            any(
                "'affected_frontends' must be a non-empty list" in e
                for e in result.errors
            )
        )


class ParityMdIdExtractionTest(unittest.TestCase):
    def test_extracts_only_from_allowed_divergences_section(self) -> None:
        md = (
            "## Other Section\n"
            "\n"
            "### `not-a-divergence`\n"
            "\n"
            "## Allowed Divergences\n"
            "\n"
            "### `real-one`\n"
            "\n"
            "### `another-real-one`\n"
            "\n"
            "## After\n"
            "\n"
            "### `also-not-a-divergence`\n"
        )
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "PARITY.md"
            p.write_text(md)
            ids = validate_parity.parity_md_divergence_ids(p)
        self.assertEqual(ids, {"real-one", "another-real-one"})

    def test_returns_none_when_file_missing(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp) / "PARITY.md"
            self.assertIsNone(validate_parity.parity_md_divergence_ids(p))


if __name__ == "__main__":
    unittest.main()
