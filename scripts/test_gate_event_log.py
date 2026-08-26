"""Tests for scripts/gate-event-log.py — envelope validation, locking, retention.

Covers the str-35vtk.18 acceptance criteria: golden valid/invalid envelopes,
concurrent complete-line appends, the exact 10000/10001 retention boundary
with sentinels, and corrupt/oversize/permission/I/O/lock failure modes.
"""

from __future__ import annotations

import fcntl
import json
import multiprocessing
import os
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parent / "gate-event-log.py"

EXIT_OK = 0
EXIT_INVALID = 64
EXIT_IOERR = 74


def golden_envelope(**overrides) -> dict:
    envelope = {
        "schema": 1,
        "event_type": "gate_pass",
        "timestamp": "2026-08-25T12:00:00Z",
        "gate": "test-standard",
        "worktree": "/home/ketan/project/shatter",
        "payload": {"duration_ms": 42, "note": "ok"},
    }
    envelope.update(overrides)
    return envelope


def _run(args: list[str], stdin: str = "", env: dict | None = None) -> subprocess.CompletedProcess[str]:
    full_env = dict(os.environ)
    if env:
        full_env.update(env)
    return subprocess.run(
        [sys.executable, str(SCRIPT), *args],
        input=stdin,
        capture_output=True,
        text=True,
        env=full_env,
        check=False,
    )


def append(path: Path, envelope: dict, env: dict | None = None) -> subprocess.CompletedProcess[str]:
    return _run(["append", "--path", str(path)], stdin=json.dumps(envelope), env=env)


def read_lines(path: Path) -> list[str]:
    return path.read_text().splitlines()


class ScriptExistsTest(unittest.TestCase):
    def test_script_exists(self) -> None:
        self.assertTrue(SCRIPT.is_file(), f"missing {SCRIPT}")


class GoldenEnvelopeTest(unittest.TestCase):
    def test_valid_envelope_is_appended(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "sub" / "gate-events.jsonl"
            result = append(path, golden_envelope())
            self.assertEqual(result.returncode, EXIT_OK, result.stderr)
            lines = read_lines(path)
            self.assertEqual(len(lines), 1)
            written = json.loads(lines[0])
            self.assertEqual(written["schema"], 1)
            self.assertEqual(written["payload"], {"duration_ms": 42, "note": "ok"})

    def test_parent_dir_and_file_permissions(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "sub" / "gate-events.jsonl"
            result = append(path, golden_envelope())
            self.assertEqual(result.returncode, EXIT_OK, result.stderr)
            parent_mode = stat.S_IMODE(path.parent.stat().st_mode)
            file_mode = stat.S_IMODE(path.stat().st_mode)
            self.assertEqual(parent_mode, 0o700)
            self.assertEqual(file_mode, 0o600)

    def test_not_json_is_invalid(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "gate-events.jsonl"
            result = _run(["append", "--path", str(path)], stdin="not json")
            self.assertEqual(result.returncode, EXIT_INVALID)
            self.assertFalse(path.exists())

    def test_json_array_is_invalid(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "gate-events.jsonl"
            result = _run(["append", "--path", str(path)], stdin="[1,2,3]")
            self.assertEqual(result.returncode, EXIT_INVALID)

    def test_bad_schema_variants_are_invalid(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "gate-events.jsonl"
            for bad_schema in (2, "1", 1.0, True, None):
                result = append(path, golden_envelope(schema=bad_schema))
                self.assertEqual(result.returncode, EXIT_INVALID, f"schema={bad_schema!r}")

    def test_bad_event_type_variants_are_invalid(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "gate-events.jsonl"
            for bad_event_type in ("", "Has-Upper", "has space", "has.dot", 123):
                result = append(path, golden_envelope(event_type=bad_event_type))
                self.assertEqual(result.returncode, EXIT_INVALID, f"event_type={bad_event_type!r}")

    def test_bad_timestamp_variants_are_invalid(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "gate-events.jsonl"
            for bad_ts in (
                "2026-08-25T12:00:00",  # missing Z
                "2026-08-25T12:00:00+00:00",  # offset instead of Z
                "2026-08-25 12:00:00Z",  # missing T
                "2026-13-01T00:00:00Z",  # invalid month
                "not-a-timestamp",
            ):
                result = append(path, golden_envelope(timestamp=bad_ts))
                self.assertEqual(result.returncode, EXIT_INVALID, f"timestamp={bad_ts!r}")

    def test_fractional_seconds_timestamp_is_valid(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "gate-events.jsonl"
            result = append(path, golden_envelope(timestamp="2026-08-25T12:00:00.123456Z"))
            self.assertEqual(result.returncode, EXIT_OK, result.stderr)

    def test_bad_gate_variants_are_invalid(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "gate-events.jsonl"
            for bad_gate in ("", "x" * 129, 123, None):
                result = append(path, golden_envelope(gate=bad_gate))
                self.assertEqual(result.returncode, EXIT_INVALID, f"gate={bad_gate!r}")

    def test_max_length_gate_is_valid(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "gate-events.jsonl"
            result = append(path, golden_envelope(gate="x" * 128))
            self.assertEqual(result.returncode, EXIT_OK, result.stderr)

    def test_bad_worktree_variants_are_invalid(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "gate-events.jsonl"
            for bad_worktree in (
                "relative/path",
                "/a/../b",
                "/a/b/",
                "/a//b",
                "",
                123,
            ):
                result = append(path, golden_envelope(worktree=bad_worktree))
                self.assertEqual(result.returncode, EXIT_INVALID, f"worktree={bad_worktree!r}")

    def test_root_worktree_is_valid(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "gate-events.jsonl"
            result = append(path, golden_envelope(worktree="/"))
            self.assertEqual(result.returncode, EXIT_OK, result.stderr)

    def test_missing_required_field_is_invalid(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "gate-events.jsonl"
            envelope = golden_envelope()
            del envelope["gate"]
            result = append(path, envelope)
            self.assertEqual(result.returncode, EXIT_INVALID)

    def test_oversize_line_is_invalid(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "gate-events.jsonl"
            envelope = golden_envelope(payload={"blob": "x" * (2 * 1024 * 1024)})
            result = append(path, envelope)
            self.assertEqual(result.returncode, EXIT_INVALID)
            self.assertFalse(path.exists())


class ConcurrentAppendTest(unittest.TestCase):
    @staticmethod
    def _append_worker(path_str: str, index: int) -> int:
        envelope = golden_envelope(event_type="concurrent_probe", gate=f"worker-{index}")
        result = subprocess.run(
            [sys.executable, str(SCRIPT), "append", "--path", path_str],
            input=json.dumps(envelope),
            capture_output=True,
            text=True,
            check=False,
        )
        return result.returncode

    def test_concurrent_appenders_produce_complete_lines(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "gate-events.jsonl"
            n_workers = 12
            with multiprocessing.Pool(n_workers) as pool:
                codes = pool.starmap(
                    self._append_worker,
                    [(str(path), i) for i in range(n_workers)],
                )
            self.assertEqual(codes, [EXIT_OK] * n_workers)

            lines = read_lines(path)
            self.assertEqual(len(lines), n_workers)
            gates_seen = set()
            for line in lines:
                obj = json.loads(line)  # raises if any line is torn/interleaved
                gates_seen.add(obj["gate"])
            self.assertEqual(len(gates_seen), n_workers)


class RetentionBoundaryTest(unittest.TestCase):
    @staticmethod
    def _write_raw_lines(path: Path, count: int) -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        with path.open("wb") as f:
            for i in range(count):
                envelope = golden_envelope(
                    event_type="sentinel", gate=f"sentinel-{i:06d}"
                )
                f.write(json.dumps(envelope).encode("utf-8") + b"\n")
        os.chmod(path, 0o600)

    def test_exactly_10000_lines_no_retention(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "gate-events.jsonl"
            self._write_raw_lines(path, 9999)
            result = append(path, golden_envelope(gate="triggering-line"))
            self.assertEqual(result.returncode, EXIT_OK, result.stderr)
            lines = read_lines(path)
            self.assertEqual(len(lines), 10000)
            first = json.loads(lines[0])
            self.assertEqual(first["gate"], "sentinel-000000")
            last = json.loads(lines[-1])
            self.assertEqual(last["gate"], "triggering-line")

    def test_10001st_line_triggers_retention_to_8000(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "gate-events.jsonl"
            self._write_raw_lines(path, 10000)
            result = append(path, golden_envelope(gate="triggering-line"))
            self.assertEqual(result.returncode, EXIT_OK, result.stderr)
            lines = read_lines(path)
            self.assertEqual(len(lines), 8000)

            last = json.loads(lines[-1])
            self.assertEqual(last["gate"], "triggering-line")

            # 10001 lines total, keep newest 8000 => drop the oldest 2001
            # (sentinel-000000 .. sentinel-002000), keep from sentinel-002001.
            first = json.loads(lines[0])
            self.assertEqual(first["gate"], "sentinel-002001")

            second_to_last = json.loads(lines[-2])
            self.assertEqual(second_to_last["gate"], "sentinel-009999")


class CorruptionTest(unittest.TestCase):
    def test_corrupt_existing_line_blocks_retention_without_truncation(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "gate-events.jsonl"
            path.parent.mkdir(parents=True, exist_ok=True)
            with path.open("wb") as f:
                for i in range(10000):
                    if i == 5000:
                        f.write(b"{not valid json\n")
                        continue
                    envelope = golden_envelope(gate=f"sentinel-{i:06d}")
                    f.write(json.dumps(envelope).encode("utf-8") + b"\n")
            os.chmod(path, 0o600)

            before_size = path.stat().st_size
            result = append(path, golden_envelope(gate="triggering-line"))
            self.assertEqual(result.returncode, EXIT_IOERR)

            # The append itself is a separate committed step and must survive;
            # only the retention rewrite is skipped on corruption.
            lines = read_lines(path)
            self.assertEqual(len(lines), 10001)
            last = json.loads(lines[-1])
            self.assertEqual(last["gate"], "triggering-line")
            self.assertGreater(path.stat().st_size, before_size)

    def test_truncated_final_line_is_separated_and_caught_by_retention(self) -> None:
        # A prior crash mid-write can leave the file's last line unterminated.
        # The appender must not silently merge the new event into that torn
        # line; it inserts a newline first so the torn line is validated (and
        # rejected) as its own line once retention runs.
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "gate-events.jsonl"
            path.parent.mkdir(parents=True, exist_ok=True)
            with path.open("wb") as f:
                for i in range(9999):
                    envelope = golden_envelope(gate=f"sentinel-{i:06d}")
                    f.write(json.dumps(envelope).encode("utf-8") + b"\n")
                f.write(b'{"schema": 1, "event_type": "torn"')  # no trailing newline, incomplete
            os.chmod(path, 0o600)

            before_size = path.stat().st_size
            result = append(path, golden_envelope(gate="triggering-line"))
            # 9999 sentinels + 1 (now newline-terminated) torn line + 1
            # triggering line = 10001 total, crossing the retention boundary;
            # the torn line fails validation during the retention pass.
            self.assertEqual(result.returncode, EXIT_IOERR)
            self.assertGreater(path.stat().st_size, before_size)
            lines = read_lines(path)
            self.assertEqual(len(lines), 10001)
            last = json.loads(lines[-1])
            self.assertEqual(last["gate"], "triggering-line")


class PermissionAndIOTest(unittest.TestCase):
    def test_unwritable_parent_directory_is_io_error(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            locked_dir = Path(tmp) / "locked"
            locked_dir.mkdir(mode=0o500)
            path = locked_dir / "sub" / "gate-events.jsonl"
            try:
                result = append(path, golden_envelope())
                self.assertEqual(result.returncode, EXIT_IOERR)
            finally:
                os.chmod(locked_dir, 0o700)

    def test_unwritable_existing_file_is_io_error(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "gate-events.jsonl"
            path.write_text("")
            os.chmod(path, 0o400)
            os.chmod(Path(tmp), 0o500)
            try:
                result = append(path, golden_envelope())
                self.assertEqual(result.returncode, EXIT_IOERR)
            finally:
                os.chmod(Path(tmp), 0o700)
                os.chmod(path, 0o600)


class LockTest(unittest.TestCase):
    def test_held_lock_times_out_as_io_error(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "gate-events.jsonl"
            path.parent.mkdir(parents=True, exist_ok=True)
            lock_path = str(path) + ".lock"
            lock_fd = os.open(lock_path, os.O_CREAT | os.O_RDWR, 0o600)
            fcntl.flock(lock_fd, fcntl.LOCK_EX)
            try:
                result = append(
                    path,
                    golden_envelope(),
                    env={"GATE_EVENT_LOG_LOCK_TIMEOUT_S": "0.3"},
                )
                self.assertEqual(result.returncode, EXIT_IOERR)
                self.assertFalse(path.exists())
            finally:
                fcntl.flock(lock_fd, fcntl.LOCK_UN)
                os.close(lock_fd)

    def test_lock_released_after_append(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "gate-events.jsonl"
            first = append(path, golden_envelope(gate="first"))
            self.assertEqual(first.returncode, EXIT_OK, first.stderr)
            second = append(
                path,
                golden_envelope(gate="second"),
                env={"GATE_EVENT_LOG_LOCK_TIMEOUT_S": "1"},
            )
            self.assertEqual(second.returncode, EXIT_OK, second.stderr)
            self.assertEqual(len(read_lines(path)), 2)


if __name__ == "__main__":
    unittest.main()
