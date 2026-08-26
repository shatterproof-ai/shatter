"""Tests for scripts/gate-pressure.py (str-35vtk.17)."""

from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import os
import sys
import tempfile
import time
import unittest
from pathlib import Path
from unittest import mock

MODULE_PATH = Path(__file__).with_name("gate-pressure.py")
SPEC = importlib.util.spec_from_file_location("gate_pressure", MODULE_PATH)
assert SPEC is not None
assert SPEC.loader is not None
gate_pressure = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = gate_pressure
SPEC.loader.exec_module(gate_pressure)


DEFAULT_MIN_MEM_BYTES = gate_pressure.DEFAULT_MIN_MEM_BYTES
DEFAULT_MIN_DISK_BYTES = gate_pressure.DEFAULT_MIN_DISK_BYTES


def _psi_line(field: str, avg10: float) -> str:
    return f"{field} avg10={avg10:.2f} avg60=0.00 avg300=0.00 total=0"


def _write_meminfo(proc_root: Path, available_kb) -> None:
    lines = ["MemTotal:       16384000 kB"]
    if available_kb is not None:
        lines.append(f"MemAvailable:   {available_kb} kB")
    (proc_root / "meminfo").write_text("\n".join(lines) + "\n")


def _write_psi(proc_root: Path, filename: str, *, some=None, full=None, raw=None) -> None:
    pressure_dir = proc_root / "pressure"
    pressure_dir.mkdir(exist_ok=True)
    path = pressure_dir / filename
    if raw is not None:
        path.write_text(raw)
        return
    lines = []
    if some is not None:
        lines.append(_psi_line("some", some))
    if full is not None:
        lines.append(_psi_line("full", full))
    path.write_text("\n".join(lines) + "\n")


def _build_healthy_proc_root(tmp_path: Path) -> Path:
    proc_root = tmp_path / "proc"
    proc_root.mkdir()
    # Comfortably above the 8 GiB default minimum.
    _write_meminfo(proc_root, available_kb=16 * 1024 * 1024)
    _write_psi(proc_root, "memory", some=0.0, full=0.0)
    _write_psi(proc_root, "io", some=0.0, full=0.0)
    _write_psi(proc_root, "cpu", some=0.0)
    return proc_root


@contextlib.contextmanager
def _env(**overrides):
    sentinel = object()
    previous = {key: os.environ.get(key, sentinel) for key in overrides}
    try:
        for key, value in overrides.items():
            if value is None:
                os.environ.pop(key, None)
            else:
                os.environ[key] = value
        yield
    finally:
        for key, prior in previous.items():
            if prior is sentinel:
                os.environ.pop(key, None)
            else:
                os.environ[key] = prior


def _run_capturing_stdout():
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        exit_code = gate_pressure.run([])
    return exit_code, buf.getvalue()


def _run_and_parse():
    exit_code, output = _run_capturing_stdout()
    return exit_code, json.loads(output)


def _signal(doc, name):
    for signal in doc["signals"]:
        if signal["name"] == name:
            return signal
    raise AssertionError(f"signal {name!r} not present in {doc!r}")


class GoldenSchemaTests(unittest.TestCase):
    def test_healthy_host_reports_ready_v1_schema(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(
                SHATTER_PROC_ROOT=str(proc_root),
                SHATTER_PRESSURE_FS_PATHS=str(fs_path),
            ):
                exit_code, doc = _run_and_parse()

        self.assertEqual(exit_code, 0)
        self.assertEqual(doc["schema_version"], 1)
        self.assertEqual(doc["status"], "ready")
        self.assertEqual(doc["platform"], "linux")
        names = {s["name"] for s in doc["signals"]}
        self.assertEqual(
            names,
            {
                "disk_free_bytes",
                "mem_available_bytes",
                "memory_full_avg10",
                "io_some_avg10",
                "cpu_some_avg10",
            },
        )
        for signal in doc["signals"]:
            self.assertIn(signal["status"], {"ready", "blocked", "unsupported"})
            self.assertEqual(signal["status"], "ready")
            self.assertIsNone(signal["reason"])


class CpuBlockingTests(unittest.TestCase):
    """New-in-this-issue: CPU saturation must block, mirroring io_some_avg10."""

    def test_cpu_alone_exceeding_max_blocks_overall_with_above_max(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            _write_psi(proc_root, "cpu", some=55.0)
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(
                SHATTER_PROC_ROOT=str(proc_root),
                SHATTER_PRESSURE_FS_PATHS=str(fs_path),
                SHATTER_GATE_MAX_CPU_SOME_AVG10="20.0",
            ):
                exit_code, doc = _run_and_parse()

        self.assertEqual(exit_code, 0)
        self.assertEqual(doc["status"], "blocked")
        cpu_signal = _signal(doc, "cpu_some_avg10")
        self.assertEqual(cpu_signal["status"], "blocked")
        self.assertEqual(cpu_signal["reason"], "above_max")
        self.assertEqual(cpu_signal["value"], 55.0)
        # Everything else stays ready -- CPU alone caused the block.
        for name in ("disk_free_bytes", "mem_available_bytes", "memory_full_avg10", "io_some_avg10"):
            self.assertEqual(_signal(doc, name)["status"], "ready")

    def test_cpu_exactly_at_threshold_is_ready(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            _write_psi(proc_root, "cpu", some=20.0)
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(
                SHATTER_PROC_ROOT=str(proc_root),
                SHATTER_PRESSURE_FS_PATHS=str(fs_path),
                SHATTER_GATE_MAX_CPU_SOME_AVG10="20.0",
            ):
                exit_code, doc = _run_and_parse()

        self.assertEqual(exit_code, 0)
        cpu_signal = _signal(doc, "cpu_some_avg10")
        self.assertEqual(cpu_signal["status"], "ready")
        self.assertIsNone(cpu_signal["reason"])

    def test_cpu_just_above_threshold_blocks(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            _write_psi(proc_root, "cpu", some=20.01)
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(
                SHATTER_PROC_ROOT=str(proc_root),
                SHATTER_PRESSURE_FS_PATHS=str(fs_path),
                SHATTER_GATE_MAX_CPU_SOME_AVG10="20.0",
            ):
                exit_code, doc = _run_and_parse()

        cpu_signal = _signal(doc, "cpu_some_avg10")
        self.assertEqual(cpu_signal["status"], "blocked")
        self.assertEqual(cpu_signal["reason"], "above_max")

    def test_cpu_default_threshold_is_20_when_unset(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            _write_psi(proc_root, "cpu", some=19.9)
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(
                SHATTER_PROC_ROOT=str(proc_root),
                SHATTER_PRESSURE_FS_PATHS=str(fs_path),
                SHATTER_GATE_MAX_CPU_SOME_AVG10=None,
            ):
                exit_code, doc = _run_and_parse()

        cpu_signal = _signal(doc, "cpu_some_avg10")
        self.assertEqual(cpu_signal["threshold"], 20.0)
        self.assertEqual(cpu_signal["status"], "ready")

    def test_cpu_invalid_override_exits_70(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(
                SHATTER_PROC_ROOT=str(proc_root),
                SHATTER_PRESSURE_FS_PATHS=str(fs_path),
                SHATTER_GATE_MAX_CPU_SOME_AVG10="not-a-number",
            ):
                exit_code, doc = _run_and_parse()

        self.assertEqual(exit_code, 70)
        self.assertEqual(doc["reason"], "invalid_override")
        self.assertEqual(doc["signal"], "SHATTER_GATE_MAX_CPU_SOME_AVG10")

    def test_cpu_negative_override_exits_70(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(
                SHATTER_PROC_ROOT=str(proc_root),
                SHATTER_PRESSURE_FS_PATHS=str(fs_path),
                SHATTER_GATE_MAX_CPU_SOME_AVG10="-1",
            ):
                exit_code, doc = _run_and_parse()

        self.assertEqual(exit_code, 70)
        self.assertEqual(doc["reason"], "invalid_override")

    def test_cpu_nan_and_inf_overrides_rejected(self):
        for bad in ("nan", "inf", "-inf"):
            with self.subTest(bad=bad):
                with tempfile.TemporaryDirectory() as tmp:
                    tmp_path = Path(tmp)
                    proc_root = _build_healthy_proc_root(tmp_path)
                    fs_path = tmp_path / "fs"
                    fs_path.mkdir()
                    with _env(
                        SHATTER_PROC_ROOT=str(proc_root),
                        SHATTER_PRESSURE_FS_PATHS=str(fs_path),
                        SHATTER_GATE_MAX_CPU_SOME_AVG10=bad,
                    ):
                        exit_code, doc = _run_and_parse()
                self.assertEqual(exit_code, 70)
                self.assertEqual(doc["reason"], "invalid_override")


class OtherOverrideBoundaryTests(unittest.TestCase):
    """Existing (unchanged) mem/disk/io behavior, kept green alongside the CPU change."""

    def test_memory_full_boundary_and_invalid_override(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            _write_psi(proc_root, "memory", some=0.0, full=1.0)
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(SHATTER_PROC_ROOT=str(proc_root), SHATTER_PRESSURE_FS_PATHS=str(fs_path)):
                exit_code, doc = _run_and_parse()
        self.assertEqual(_signal(doc, "memory_full_avg10")["status"], "ready")

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(
                SHATTER_PROC_ROOT=str(proc_root),
                SHATTER_PRESSURE_FS_PATHS=str(fs_path),
                SHATTER_GATE_MAX_MEMORY_FULL_AVG10="oops",
            ):
                exit_code, doc = _run_and_parse()
        self.assertEqual(exit_code, 70)
        self.assertEqual(doc["signal"], "SHATTER_GATE_MAX_MEMORY_FULL_AVG10")

    def test_io_some_boundary_and_invalid_override(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            _write_psi(proc_root, "io", some=20.0, full=0.0)
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(SHATTER_PROC_ROOT=str(proc_root), SHATTER_PRESSURE_FS_PATHS=str(fs_path)):
                exit_code, doc = _run_and_parse()
        self.assertEqual(_signal(doc, "io_some_avg10")["status"], "ready")

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(
                SHATTER_PROC_ROOT=str(proc_root),
                SHATTER_PRESSURE_FS_PATHS=str(fs_path),
                SHATTER_GATE_MAX_IO_SOME_AVG10="oops",
            ):
                exit_code, doc = _run_and_parse()
        self.assertEqual(exit_code, 70)
        self.assertEqual(doc["signal"], "SHATTER_GATE_MAX_IO_SOME_AVG10")

    def test_min_mem_bytes_boundary_and_invalid_override(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            _write_meminfo(proc_root, available_kb=DEFAULT_MIN_MEM_BYTES // 1024)
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(SHATTER_PROC_ROOT=str(proc_root), SHATTER_PRESSURE_FS_PATHS=str(fs_path)):
                exit_code, doc = _run_and_parse()
        self.assertEqual(_signal(doc, "mem_available_bytes")["status"], "ready")

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            _write_meminfo(proc_root, available_kb=(DEFAULT_MIN_MEM_BYTES // 1024) - 1)
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(SHATTER_PROC_ROOT=str(proc_root), SHATTER_PRESSURE_FS_PATHS=str(fs_path)):
                exit_code, doc = _run_and_parse()
        signal = _signal(doc, "mem_available_bytes")
        self.assertEqual(signal["status"], "blocked")
        self.assertEqual(signal["reason"], "below_min")
        self.assertEqual(doc["status"], "blocked")

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(
                SHATTER_PROC_ROOT=str(proc_root),
                SHATTER_PRESSURE_FS_PATHS=str(fs_path),
                SHATTER_GATE_MIN_MEM_BYTES="not-an-int",
            ):
                exit_code, doc = _run_and_parse()
        self.assertEqual(exit_code, 70)
        self.assertEqual(doc["signal"], "SHATTER_GATE_MIN_MEM_BYTES")

    def test_min_disk_bytes_invalid_override(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(
                SHATTER_PROC_ROOT=str(proc_root),
                SHATTER_PRESSURE_FS_PATHS=str(fs_path),
                SHATTER_GATE_MIN_DISK_BYTES="-5",
            ):
                exit_code, doc = _run_and_parse()
        self.assertEqual(exit_code, 70)
        self.assertEqual(doc["signal"], "SHATTER_GATE_MIN_DISK_BYTES")


class DiskDeviceDedupTests(unittest.TestCase):
    def test_duplicate_device_paths_collapse_to_one(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            shared = tmp_path / "shared"
            shared.mkdir()
            nested = shared / "a" / "b"
            nested.mkdir(parents=True)
            fs_paths = os.pathsep.join([str(shared), str(nested), str(shared)])
            with _env(SHATTER_PROC_ROOT=str(proc_root), SHATTER_PRESSURE_FS_PATHS=fs_paths):
                exit_code, doc = _run_and_parse()

        disk_signal = _signal(doc, "disk_free_bytes")
        self.assertEqual(disk_signal["detail"]["unique_devices"], 1)
        self.assertEqual(len(disk_signal["detail"]["paths_checked"]), 3)

    def test_ancestor_fallback_for_missing_path(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            missing = tmp_path / "does" / "not" / "exist" / "yet"
            with _env(SHATTER_PROC_ROOT=str(proc_root), SHATTER_PRESSURE_FS_PATHS=str(missing)):
                exit_code, doc = _run_and_parse()

            self.assertEqual(exit_code, 0)
            disk_signal = _signal(doc, "disk_free_bytes")
            resolved = disk_signal["detail"]["paths_checked"][0]["resolved"]
            self.assertTrue(os.path.exists(resolved))
            self.assertNotEqual(resolved, str(missing))

    def test_disk_below_min_blocks_overall(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(
                SHATTER_PROC_ROOT=str(proc_root),
                SHATTER_PRESSURE_FS_PATHS=str(fs_path),
                SHATTER_GATE_MIN_DISK_BYTES=str(2**62),
            ):
                exit_code, doc = _run_and_parse()

        disk_signal = _signal(doc, "disk_free_bytes")
        self.assertEqual(disk_signal["status"], "blocked")
        self.assertEqual(disk_signal["reason"], "below_min")
        self.assertEqual(doc["status"], "blocked")

    def test_statvfs_failure_exits_70(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(SHATTER_PROC_ROOT=str(proc_root), SHATTER_PRESSURE_FS_PATHS=str(fs_path)):
                with mock.patch.object(
                    gate_pressure.os, "statvfs", side_effect=OSError("boom")
                ):
                    exit_code, doc = _run_and_parse()

        self.assertEqual(exit_code, 70)
        self.assertEqual(doc["reason"], "unreadable")
        self.assertEqual(doc["signal"], "disk_free_bytes")


class DefaultPathsIgnoreStalePwdTests(unittest.TestCase):
    """Regression: os.getcwd() must win over a stale inherited $PWD.

    $PWD is shell-maintained and is not refreshed by a parent process's
    os.chdir()/set_current_dir() before spawning this script as a
    subprocess -- a stale PWD must not steer the default disk-check path
    away from the real current working directory.
    """

    def test_resolve_fs_paths_prefers_getcwd_over_stale_pwd(self):
        real_cwd = os.getcwd()
        with _env(SHATTER_PRESSURE_FS_PATHS=None, PWD="/definitely/not/the/real/cwd"):
            paths = gate_pressure.resolve_fs_paths()
        self.assertEqual(paths[0], real_cwd)
        self.assertNotEqual(paths[0], "/definitely/not/the/real/cwd")

    def test_run_reports_disk_for_real_cwd_not_stale_pwd(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            real_cwd_dir = tmp_path / "real-cwd"
            real_cwd_dir.mkdir()
            stale_pwd_dir = tmp_path / "stale-pwd"
            stale_pwd_dir.mkdir()

            previous_cwd = os.getcwd()
            os.chdir(real_cwd_dir)
            try:
                with _env(
                    SHATTER_PROC_ROOT=str(proc_root),
                    SHATTER_PRESSURE_FS_PATHS=None,
                    PWD=str(stale_pwd_dir),
                    CARGO_TARGET_DIR=str(real_cwd_dir),
                    SCCACHE_DIR=str(real_cwd_dir),
                    TMPDIR=str(real_cwd_dir),
                ):
                    exit_code, doc = _run_and_parse()
            finally:
                os.chdir(previous_cwd)

        self.assertEqual(exit_code, 0)
        disk_signal = _signal(doc, "disk_free_bytes")
        checked_inputs = {entry["input"] for entry in disk_signal["detail"]["paths_checked"]}
        self.assertIn(str(real_cwd_dir), checked_inputs)
        self.assertNotIn(str(stale_pwd_dir), checked_inputs)


class MemInfoErrorTests(unittest.TestCase):
    def test_missing_meminfo_exits_70(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = tmp_path / "proc"
            proc_root.mkdir()
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(SHATTER_PROC_ROOT=str(proc_root), SHATTER_PRESSURE_FS_PATHS=str(fs_path)):
                exit_code, doc = _run_and_parse()
        self.assertEqual(exit_code, 70)
        self.assertEqual(doc["reason"], "missing")
        self.assertEqual(doc["signal"], "mem_available_bytes")

    def test_malformed_meminfo_missing_field_exits_70(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = tmp_path / "proc"
            proc_root.mkdir()
            (proc_root / "meminfo").write_text("MemTotal: 16384000 kB\n")
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(SHATTER_PROC_ROOT=str(proc_root), SHATTER_PRESSURE_FS_PATHS=str(fs_path)):
                exit_code, doc = _run_and_parse()
        self.assertEqual(exit_code, 70)
        self.assertEqual(doc["reason"], "malformed")

    def test_malformed_meminfo_non_numeric_value_exits_70(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = tmp_path / "proc"
            proc_root.mkdir()
            (proc_root / "meminfo").write_text("MemAvailable: notanumber kB\n")
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(SHATTER_PROC_ROOT=str(proc_root), SHATTER_PRESSURE_FS_PATHS=str(fs_path)):
                exit_code, doc = _run_and_parse()
        self.assertEqual(exit_code, 70)
        self.assertEqual(doc["reason"], "malformed")

    def test_unreadable_meminfo_exits_70(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            meminfo_path = str(proc_root / "meminfo")

            real_open = open

            def fake_open(path, *args, **kwargs):
                if path == meminfo_path:
                    raise PermissionError("denied")
                return real_open(path, *args, **kwargs)

            with _env(SHATTER_PROC_ROOT=str(proc_root), SHATTER_PRESSURE_FS_PATHS=str(fs_path)):
                with mock.patch("builtins.open", side_effect=fake_open):
                    exit_code, doc = _run_and_parse()
        self.assertEqual(exit_code, 70)
        self.assertEqual(doc["reason"], "unreadable")


class PsiErrorTests(unittest.TestCase):
    def test_missing_psi_file_is_unsupported_nonblocking(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = tmp_path / "proc"
            proc_root.mkdir()
            _write_meminfo(proc_root, available_kb=16 * 1024 * 1024)
            # No pressure/ directory at all -- every PSI signal is unsupported.
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(SHATTER_PROC_ROOT=str(proc_root), SHATTER_PRESSURE_FS_PATHS=str(fs_path)):
                exit_code, doc = _run_and_parse()

        self.assertEqual(exit_code, 0)
        self.assertEqual(doc["status"], "ready")
        for name in ("memory_full_avg10", "io_some_avg10", "cpu_some_avg10"):
            signal = _signal(doc, name)
            self.assertEqual(signal["status"], "unsupported")
            self.assertEqual(signal["reason"], "unsupported")
            self.assertIsNone(signal["value"])

    def test_malformed_present_psi_exits_70(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            _write_psi(proc_root, "cpu", raw="garbage no avg10 here\n")
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(SHATTER_PROC_ROOT=str(proc_root), SHATTER_PRESSURE_FS_PATHS=str(fs_path)):
                exit_code, doc = _run_and_parse()

        self.assertEqual(exit_code, 70)
        self.assertEqual(doc["reason"], "malformed")
        self.assertEqual(doc["signal"], "cpu_some_avg10")

    def test_malformed_present_psi_non_numeric_avg10_exits_70(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            _write_psi(proc_root, "cpu", raw="some avg10=oops avg60=0.00 avg300=0.00 total=0\n")
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(SHATTER_PROC_ROOT=str(proc_root), SHATTER_PRESSURE_FS_PATHS=str(fs_path)):
                exit_code, doc = _run_and_parse()

        self.assertEqual(exit_code, 70)
        self.assertEqual(doc["reason"], "malformed")


class NonLinuxTests(unittest.TestCase):
    def test_non_linux_all_proc_signals_unsupported_status_from_disk(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(SHATTER_PROC_ROOT=str(proc_root), SHATTER_PRESSURE_FS_PATHS=str(fs_path)):
                with mock.patch.object(gate_pressure.sys, "platform", "darwin"):
                    exit_code, doc = _run_and_parse()

        self.assertEqual(exit_code, 0)
        self.assertEqual(doc["platform"], "other")
        self.assertEqual(doc["status"], "ready")
        for name in ("mem_available_bytes", "memory_full_avg10", "io_some_avg10", "cpu_some_avg10"):
            signal = _signal(doc, name)
            self.assertEqual(signal["status"], "unsupported")
        # Disk is still evaluated on non-Linux hosts.
        self.assertEqual(_signal(doc, "disk_free_bytes")["status"], "ready")

    def test_non_linux_status_derives_from_disk_when_disk_blocked(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(
                SHATTER_PROC_ROOT=str(proc_root),
                SHATTER_PRESSURE_FS_PATHS=str(fs_path),
                SHATTER_GATE_MIN_DISK_BYTES=str(2**62),
            ):
                with mock.patch.object(gate_pressure.sys, "platform", "win32"):
                    exit_code, doc = _run_and_parse()

        self.assertEqual(doc["platform"], "other")
        self.assertEqual(doc["status"], "blocked")

    def test_non_linux_invalid_override_still_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(
                SHATTER_PROC_ROOT=str(proc_root),
                SHATTER_PRESSURE_FS_PATHS=str(fs_path),
                SHATTER_GATE_MAX_CPU_SOME_AVG10="garbage",
            ):
                with mock.patch.object(gate_pressure.sys, "platform", "darwin"):
                    exit_code, doc = _run_and_parse()

        self.assertEqual(exit_code, 70)
        self.assertEqual(doc["reason"], "invalid_override")


class TimeoutTests(unittest.TestCase):
    def test_run_completes_within_two_seconds(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            proc_root = _build_healthy_proc_root(tmp_path)
            fs_path = tmp_path / "fs"
            fs_path.mkdir()
            with _env(SHATTER_PROC_ROOT=str(proc_root), SHATTER_PRESSURE_FS_PATHS=str(fs_path)):
                start = time.monotonic()
                exit_code, _doc = _run_and_parse()
                elapsed = time.monotonic() - start

        self.assertEqual(exit_code, 0)
        self.assertLess(elapsed, 2.0)


class RealHostSmokeTest(unittest.TestCase):
    """No injected roots: exercises the script against the real machine."""

    def test_real_proc_root_produces_valid_document_or_hard_error(self):
        exit_code, output = _run_capturing_stdout()
        doc = json.loads(output)
        self.assertIn(exit_code, (0, 70))
        if exit_code == 0:
            self.assertIn(doc["status"], {"ready", "blocked"})
            self.assertEqual(doc["schema_version"], 1)
        else:
            self.assertEqual(doc["status"], "error")


if __name__ == "__main__":
    unittest.main()
