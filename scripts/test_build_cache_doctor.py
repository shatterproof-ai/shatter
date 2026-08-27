from __future__ import annotations

import importlib.util
import json
import os
import stat
import sys
import tempfile
import time
import unittest
from dataclasses import asdict
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("build-cache-doctor.py")
SPEC = importlib.util.spec_from_file_location("build_cache_doctor", MODULE_PATH)
assert SPEC is not None
assert SPEC.loader is not None
doctor = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = doctor
SPEC.loader.exec_module(doctor)


def make_executable(path: Path, contents: str = "#!/bin/sh\nexit 0\n") -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(contents, encoding="utf-8")
    path.chmod(path.stat().st_mode | stat.S_IEXEC | stat.S_IXGRP | stat.S_IXOTH)
    return path


def make_unexecutable(path: Path) -> Path:
    path.write_text("not executable\n", encoding="utf-8")
    path.chmod(0o644)
    return path


def write_toml(path: Path, text: str) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    return path


def fake_stats(*_a, cache_dir: str | None = None, **_k) -> doctor.StatsResult:
    return doctor.StatsResult(ok=True, timed_out=False, stats={"stats": {}}, cache_dir=cache_dir)


def failing_stats(*_a, **_k) -> doctor.StatsResult:
    return doctor.StatsResult(ok=False, timed_out=False, stats=None, cache_dir=None)


def timeout_stats(*_a, **_k) -> doctor.StatsResult:
    return doctor.StatsResult(ok=False, timed_out=True, stats=None, cache_dir=None)


class DoctorReportTestBase(unittest.TestCase):
    def setUp(self) -> None:
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.home = self.root / "home"
        self.cargo_home = self.root / "cargo_home"
        self.repo_root = self.root / "repo"
        self.bin_a = self.root / "bin_a"
        self.bin_b = self.root / "bin_b"
        for d in (self.home, self.cargo_home, self.repo_root, self.bin_a, self.bin_b):
            d.mkdir(parents=True, exist_ok=True)

    def build(self, *, env=None, cargo_home=None, path_dirs=None, run_stats=fake_stats):
        return doctor.build_report(
            env=env or {},
            home=self.home,
            cargo_home=self.cargo_home if cargo_home is None else cargo_home,
            repo_root=self.repo_root,
            path_dirs=path_dirs if path_dirs is not None else [str(self.bin_a), str(self.bin_b)],
            run_stats=run_stats,
        )


class WrapperPrecedenceTest(DoctorReportTestBase):
    def test_environment_wins_over_user_and_repo_config(self) -> None:
        make_executable(self.bin_a / "sccache")
        write_toml(self.cargo_home / "config.toml", '[build]\nrustc-wrapper = "not-sccache"\n')
        write_toml(self.repo_root / ".cargo" / "config.toml", '[build]\nrustc-wrapper = "also-not-sccache"\n')

        report = self.build(env={"RUSTC_WRAPPER": "sccache"})

        self.assertEqual(report.wrapper.source, "environment")
        self.assertEqual(report.wrapper.configured, "sccache")
        self.assertEqual(report.wrapper.resolved_path, str(self.bin_a / "sccache"))

    def test_user_config_wins_over_repo_config(self) -> None:
        make_executable(self.bin_a / "sccache")
        write_toml(self.cargo_home / "config.toml", '[build]\nrustc-wrapper = "sccache"\n')
        write_toml(self.repo_root / ".cargo" / "config.toml", '[build]\nrustc-wrapper = "other-wrapper"\n')

        report = self.build(env={})

        self.assertEqual(report.wrapper.source, "user_config")
        self.assertEqual(report.wrapper.configured, "sccache")

    def test_repo_config_used_when_no_env_or_user_config(self) -> None:
        make_executable(self.bin_a / "sccache")
        write_toml(self.repo_root / ".cargo" / "config.toml", '[build]\nrustc-wrapper = "sccache"\n')

        report = self.build(env={})

        self.assertEqual(report.wrapper.source, "repo_config")

    def test_no_wrapper_configured_anywhere(self) -> None:
        report = self.build(env={})
        self.assertEqual(report.wrapper.source, "none")
        self.assertIsNone(report.wrapper.configured)
        self.assertIsNone(report.wrapper.resolved_path)
        self.assertNotIn(doctor.DIAG_WRAPPER_MISSING, report.diagnostics)

    def test_configured_wrapper_missing_from_path(self) -> None:
        report = self.build(env={"RUSTC_WRAPPER": "sccache"})
        self.assertIsNone(report.wrapper.resolved_path)
        self.assertIn(doctor.DIAG_WRAPPER_MISSING, report.diagnostics)

    def test_configured_wrapper_unexecutable(self) -> None:
        make_unexecutable(self.bin_a / "sccache")
        report = self.build(env={"RUSTC_WRAPPER": "sccache"})
        self.assertIn(doctor.DIAG_WRAPPER_UNEXECUTABLE, report.diagnostics)

    def test_configured_wrapper_not_named_sccache(self) -> None:
        make_executable(self.bin_a / "ccache")
        report = self.build(env={"RUSTC_WRAPPER": "ccache"})
        self.assertIn(doctor.DIAG_WRAPPER_NOT_SCCACHE, report.diagnostics)

    def test_absolute_wrapper_path(self) -> None:
        absolute = make_executable(self.root / "custom" / "sccache")
        report = self.build(env={"RUSTC_WRAPPER": str(absolute)})
        self.assertEqual(report.wrapper.resolved_path, str(absolute))


class LinkerPrecedenceTest(DoctorReportTestBase):
    def test_environment_rustflags_win(self) -> None:
        make_executable(self.bin_a / "mold")
        write_toml(
            self.repo_root / ".cargo" / "config.toml",
            '[target.x86_64-unknown-linux-gnu]\nrustflags = ["-C", "link-arg=-fuse-ld=lld"]\n',
        )

        report = self.build(env={doctor.TARGET_RUSTFLAGS_ENV: "-C link-arg=-fuse-ld=mold"})

        self.assertEqual(report.linker.source, "environment")
        self.assertEqual(report.linker.name, "mold")

    def test_user_config_wins_over_repo_config(self) -> None:
        make_executable(self.bin_a / "mold")
        write_toml(
            self.cargo_home / "config.toml",
            '[target.x86_64-unknown-linux-gnu]\nrustflags = ["-C", "link-arg=-fuse-ld=mold"]\n',
        )
        write_toml(
            self.repo_root / ".cargo" / "config.toml",
            '[target.x86_64-unknown-linux-gnu]\nrustflags = ["-C", "link-arg=-fuse-ld=lld"]\n',
        )

        report = self.build(env={})

        self.assertEqual(report.linker.source, "user_config")
        self.assertEqual(report.linker.name, "mold")

    def test_repo_config_array_form_parsed(self) -> None:
        make_executable(self.bin_a / "ld.lld")
        write_toml(
            self.repo_root / ".cargo" / "config.toml",
            '[target.x86_64-unknown-linux-gnu]\nrustflags = ["-C", "link-arg=-fuse-ld=lld"]\n',
        )

        report = self.build(env={})

        self.assertEqual(report.linker.source, "repo_config")
        self.assertEqual(report.linker.name, "lld")
        self.assertEqual(report.linker.resolved_path, str(self.bin_a / "ld.lld"))

    def test_other_linker_name_normalized(self) -> None:
        make_executable(self.bin_a / "gold")
        write_toml(
            self.repo_root / ".cargo" / "config.toml",
            '[target.x86_64-unknown-linux-gnu]\nrustflags = "-C link-arg=-fuse-ld=gold"\n',
        )

        report = self.build(env={})

        self.assertEqual(report.linker.name, "other")
        self.assertEqual(report.linker.configured, "gold")

    def test_configured_linker_missing(self) -> None:
        write_toml(
            self.repo_root / ".cargo" / "config.toml",
            '[target.x86_64-unknown-linux-gnu]\nrustflags = ["-C", "link-arg=-fuse-ld=mold"]\n',
        )
        report = self.build(env={})
        self.assertIn(doctor.DIAG_LINKER_MISSING, report.diagnostics)
        self.assertIsNone(report.linker.resolved_path)

    def test_configured_linker_unexecutable(self) -> None:
        make_unexecutable(self.bin_a / "mold")
        write_toml(
            self.repo_root / ".cargo" / "config.toml",
            '[target.x86_64-unknown-linux-gnu]\nrustflags = ["-C", "link-arg=-fuse-ld=mold"]\n',
        )
        report = self.build(env={})
        self.assertIn(doctor.DIAG_LINKER_UNEXECUTABLE, report.diagnostics)

    def test_discovery_prefers_mold_over_lld(self) -> None:
        make_executable(self.bin_a / "mold")
        make_executable(self.bin_a / "ld.lld")

        report = self.build(env={})

        self.assertEqual(report.linker.name, "mold")
        self.assertEqual(report.linker.source, "none")
        self.assertIsNone(report.linker.configured)
        self.assertIsNotNone(report.linker.resolved_path)

    def test_discovery_falls_back_to_lld(self) -> None:
        make_executable(self.bin_a / "ld.lld")

        report = self.build(env={})

        self.assertEqual(report.linker.name, "lld")

    def test_no_linker_discoverable(self) -> None:
        report = self.build(env={})
        self.assertEqual(report.linker.name, "none")
        self.assertIsNone(report.linker.resolved_path)
        self.assertEqual(report.linker.source, "none")
        self.assertNotIn(doctor.DIAG_LINKER_MISSING, report.diagnostics)


class SccacheStateTest(DoctorReportTestBase):
    def test_missing_when_not_installed_and_not_wrapped(self) -> None:
        report = self.build(env={})
        self.assertEqual(report.sccache.state, "missing")
        self.assertIn(doctor.DIAG_SCCACHE_MISSING, report.diagnostics)

    def test_active_when_wrapper_resolves_to_sccache(self) -> None:
        make_executable(self.bin_a / "sccache")
        report = self.build(env={"RUSTC_WRAPPER": "sccache"})
        self.assertEqual(report.sccache.state, "active")
        self.assertEqual(report.sccache.binary_path, str(self.bin_a / "sccache"))

    def test_installed_inactive_when_sccache_on_path_but_not_wired(self) -> None:
        make_executable(self.bin_a / "sccache")
        report = self.build(env={})
        self.assertEqual(report.sccache.state, "installed_inactive")
        self.assertNotIn(doctor.DIAG_WRAPPER_MISSING, report.diagnostics)

    def test_wedged_on_stats_timeout(self) -> None:
        make_executable(self.bin_a / "sccache")
        report = self.build(env={"RUSTC_WRAPPER": "sccache"}, run_stats=timeout_stats)
        self.assertEqual(report.sccache.state, "wedged")
        self.assertIn(doctor.DIAG_SCCACHE_WEDGED, report.diagnostics)
        self.assertEqual(report.status, "partial")

    def test_error_on_stats_failure(self) -> None:
        make_executable(self.bin_a / "sccache")
        report = self.build(env={"RUSTC_WRAPPER": "sccache"}, run_stats=failing_stats)
        self.assertEqual(report.sccache.state, "error")
        self.assertIn(doctor.DIAG_SCCACHE_STATS_ERROR, report.diagnostics)
        self.assertEqual(report.status, "error")

    def test_cache_dir_precedence_env_wins(self) -> None:
        make_executable(self.bin_a / "sccache")
        custom_dir = self.root / "custom-cache"
        custom_dir.mkdir()

        report = self.build(
            env={"RUSTC_WRAPPER": "sccache", "SCCACHE_DIR": str(custom_dir)},
            run_stats=lambda *_a, **_k: fake_stats(cache_dir=str(self.root / "from-stats")),
        )

        self.assertEqual(report.sccache.cache_dir, str(custom_dir))

    def test_cache_dir_from_stats_when_no_env(self) -> None:
        make_executable(self.bin_a / "sccache")
        stats_dir = self.root / "from-stats"
        stats_dir.mkdir()

        report = self.build(
            env={"RUSTC_WRAPPER": "sccache"},
            run_stats=lambda *_a, **_k: fake_stats(cache_dir=str(stats_dir)),
        )

        self.assertEqual(report.sccache.cache_dir, str(stats_dir))

    def test_cache_dir_default_fallback(self) -> None:
        make_executable(self.bin_a / "sccache")
        report = self.build(env={"RUSTC_WRAPPER": "sccache"})
        self.assertEqual(report.sccache.cache_dir, str(self.home / ".cache" / "sccache"))

    def test_cache_size_counts_regular_files_only(self) -> None:
        make_executable(self.bin_a / "sccache")
        cache_dir = self.root / "cache"
        cache_dir.mkdir()
        (cache_dir / "a.bin").write_bytes(b"1234567890")
        subdir = cache_dir / "sub"
        subdir.mkdir()
        (subdir / "b.bin").write_bytes(b"12345")
        (cache_dir / "link.bin").symlink_to(cache_dir / "a.bin")

        report = self.build(
            env={"RUSTC_WRAPPER": "sccache", "SCCACHE_DIR": str(cache_dir)},
        )

        self.assertEqual(report.sccache.cache_size_bytes, 15)

    def test_cache_size_null_when_dir_absent(self) -> None:
        make_executable(self.bin_a / "sccache")
        report = self.build(
            env={"RUSTC_WRAPPER": "sccache", "SCCACHE_DIR": str(self.root / "does-not-exist")},
        )
        self.assertIsNone(report.sccache.cache_size_bytes)

    def test_cache_size_null_when_dir_exists_but_unreadable(self) -> None:
        make_executable(self.bin_a / "sccache")
        cache_dir = self.root / "locked-cache"
        cache_dir.mkdir()
        (cache_dir / "a.bin").write_bytes(b"1234567890")
        original_mode = cache_dir.stat().st_mode
        cache_dir.chmod(0o000)
        self.addCleanup(cache_dir.chmod, original_mode)

        try:
            report = self.build(
                env={"RUSTC_WRAPPER": "sccache", "SCCACHE_DIR": str(cache_dir)},
            )
        finally:
            cache_dir.chmod(original_mode)

        self.assertIsNone(report.sccache.cache_size_bytes)


class BudgetsTest(DoctorReportTestBase):
    def test_cargo_build_jobs_env_wins_over_repo_config(self) -> None:
        write_toml(self.repo_root / ".cargo" / "config.toml", "[build]\njobs = 8\n")
        report = self.build(env={"CARGO_BUILD_JOBS": "3"})
        self.assertEqual(report.budgets.cargo_build_jobs, "3")
        self.assertEqual(report.budgets.cargo_source, "gate_environment")

    def test_cargo_build_jobs_repo_config_fallback(self) -> None:
        write_toml(self.repo_root / ".cargo" / "config.toml", "[build]\njobs = 8\n")
        report = self.build(env={})
        self.assertEqual(report.budgets.cargo_build_jobs, "8")
        self.assertEqual(report.budgets.cargo_source, "repo_config")

    def test_cargo_build_jobs_default_when_unset(self) -> None:
        report = self.build(env={})
        self.assertIsNone(report.budgets.cargo_build_jobs)
        self.assertEqual(report.budgets.cargo_source, "default")

    def test_shatter_cpu_budget_env(self) -> None:
        report = self.build(env={"SHATTER_CPU_BUDGET": "4"})
        self.assertEqual(report.budgets.shatter_cpu_budget, "4")
        self.assertEqual(report.budgets.shatter_source, "gate_environment")

    def test_shatter_cpu_budget_none_when_unset(self) -> None:
        report = self.build(env={})
        self.assertIsNone(report.budgets.shatter_cpu_budget)
        self.assertEqual(report.budgets.shatter_source, "none")


class TopStatusTest(DoctorReportTestBase):
    def test_accelerated_requires_active_sccache_and_configured_linker(self) -> None:
        make_executable(self.bin_a / "sccache")
        make_executable(self.bin_a / "mold")
        write_toml(
            self.repo_root / ".cargo" / "config.toml",
            '[target.x86_64-unknown-linux-gnu]\nrustflags = ["-C", "link-arg=-fuse-ld=mold"]\n',
        )
        report = self.build(env={"RUSTC_WRAPPER": "sccache"})
        self.assertEqual(report.status, "accelerated")

    def test_partial_when_only_sccache_active(self) -> None:
        make_executable(self.bin_a / "sccache")
        report = self.build(env={"RUSTC_WRAPPER": "sccache"})
        self.assertEqual(report.status, "partial")

    def test_partial_when_only_linker_configured(self) -> None:
        make_executable(self.bin_a / "mold")
        write_toml(
            self.repo_root / ".cargo" / "config.toml",
            '[target.x86_64-unknown-linux-gnu]\nrustflags = ["-C", "link-arg=-fuse-ld=mold"]\n',
        )
        report = self.build(env={})
        self.assertEqual(report.status, "partial")

    def test_partial_when_linker_only_discovered(self) -> None:
        make_executable(self.bin_a / "mold")
        report = self.build(env={})
        self.assertEqual(report.status, "partial")

    def test_partial_when_sccache_installed_inactive(self) -> None:
        make_executable(self.bin_a / "sccache")
        report = self.build(env={})
        self.assertEqual(report.status, "partial")

    def test_accelerated_not_satisfied_by_discovery_alone(self) -> None:
        make_executable(self.bin_a / "sccache")
        make_executable(self.bin_a / "mold")
        report = self.build(env={"RUSTC_WRAPPER": "sccache"})
        self.assertNotEqual(report.status, "accelerated")
        self.assertEqual(report.status, "partial")

    def test_inactive_when_nothing_present(self) -> None:
        report = self.build(env={})
        self.assertEqual(report.status, "inactive")

    def test_error_on_malformed_repo_config(self) -> None:
        write_toml(self.repo_root / ".cargo" / "config.toml", "not [valid toml\n")
        report = self.build(env={})
        self.assertEqual(report.status, "error")
        self.assertIn(doctor.DIAG_INVALID_CONFIG, report.diagnostics)

    def test_error_on_malformed_user_config(self) -> None:
        write_toml(self.cargo_home / "config.toml", "not [valid toml\n")
        report = self.build(env={})
        self.assertEqual(report.status, "error")
        self.assertIn(doctor.DIAG_INVALID_CONFIG, report.diagnostics)

    def test_error_on_structurally_wrong_repo_target_table(self) -> None:
        # Syntactically valid TOML, but `target` is a string instead of a
        # table -- must not raise AttributeError out of get_target_rustflags.
        write_toml(self.repo_root / ".cargo" / "config.toml", 'target = "oops"\n')
        report = self.build(env={})
        self.assertEqual(report.status, "error")
        self.assertIn(doctor.DIAG_INVALID_CONFIG, report.diagnostics)

    def test_error_on_structurally_wrong_user_build_table(self) -> None:
        # Syntactically valid TOML, but `build` is a string instead of a
        # table -- must not raise AttributeError out of resolve_wrapper /
        # resolve_cargo_budget.
        write_toml(self.cargo_home / "config.toml", 'build = "x"\n')
        report = self.build(env={})
        self.assertEqual(report.status, "error")
        self.assertIn(doctor.DIAG_INVALID_CONFIG, report.diagnostics)

    def test_error_on_structurally_wrong_target_triple_table(self) -> None:
        write_toml(
            self.repo_root / ".cargo" / "config.toml",
            "[target]\nx86_64-unknown-linux-gnu = \"oops\"\n",
        )
        report = self.build(env={})
        self.assertEqual(report.status, "error")
        self.assertIn(doctor.DIAG_INVALID_CONFIG, report.diagnostics)

    def test_valid_config_with_unrelated_non_table_keys_is_not_flagged(self) -> None:
        write_toml(
            self.repo_root / ".cargo" / "config.toml",
            '[build]\njobs = 4\nsomething-else = "fine"\n',
        )
        report = self.build(env={})
        self.assertNotEqual(report.status, "error")
        self.assertNotIn(doctor.DIAG_INVALID_CONFIG, report.diagnostics)


class DiagnosticOrderingTest(DoctorReportTestBase):
    def test_diagnostics_are_deduplicated_and_ordered(self) -> None:
        write_toml(self.repo_root / ".cargo" / "config.toml", "not [valid toml\n")
        report = self.build(env={"RUSTC_WRAPPER": "sccache"})
        self.assertEqual(
            report.diagnostics,
            [doctor.DIAG_WRAPPER_MISSING, doctor.DIAG_SCCACHE_MISSING, doctor.DIAG_INVALID_CONFIG],
        )


class SnapshotTest(DoctorReportTestBase):
    def test_full_report_shape_snapshot(self) -> None:
        make_executable(self.bin_a / "sccache")
        make_executable(self.bin_a / "mold")
        write_toml(
            self.repo_root / ".cargo" / "config.toml",
            '[target.x86_64-unknown-linux-gnu]\nrustflags = ["-C", "link-arg=-fuse-ld=mold"]\n',
        )

        report = self.build(
            env={"RUSTC_WRAPPER": "sccache"},
            run_stats=lambda *_a, **_k: fake_stats(cache_dir=str(self.root / "cache")),
        )
        payload = asdict(report)

        self.assertEqual(
            payload,
            {
                "schema": 1,
                "status": "accelerated",
                "wrapper": {
                    "configured": "sccache",
                    "resolved_path": str(self.bin_a / "sccache"),
                    "source": "environment",
                },
                "linker": {
                    "name": "mold",
                    "configured": "mold",
                    "resolved_path": str(self.bin_a / "mold"),
                    "source": "repo_config",
                },
                "sccache": {
                    "state": "active",
                    "binary_path": str(self.bin_a / "sccache"),
                    "cache_dir": str(self.root / "cache"),
                    "cache_size_bytes": None,
                    "stats": {"stats": {}},
                },
                "budgets": {
                    "cargo_build_jobs": None,
                    "cargo_source": "default",
                    "shatter_cpu_budget": None,
                    "shatter_source": "none",
                },
                "diagnostics": [],
            },
        )


class CliExitCodeTest(unittest.TestCase):
    def test_bad_arguments_exit_64(self) -> None:
        with self.assertRaises(SystemExit) as ctx:
            doctor.build_arg_parser().parse_args(["--not-a-real-flag"])
        self.assertEqual(ctx.exception.code, doctor.EXIT_ARGS)

    def test_help_flag_does_not_use_error_exit_code(self) -> None:
        with self.assertRaises(SystemExit) as ctx:
            doctor.build_arg_parser().parse_args(["--help"])
        self.assertEqual(ctx.exception.code, 0)

    def test_main_exits_ok_by_default(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            old_cwd = os.getcwd()
            os.chdir(tmp)
            try:
                exit_code = doctor.main(["--json"])
            finally:
                os.chdir(old_cwd)
        self.assertIn(exit_code, (doctor.EXIT_OK, doctor.EXIT_ERROR))

    def test_strict_flag_forces_exit_2_when_not_accelerated(self) -> None:
        real_build_report = doctor.build_report
        try:
            doctor.build_report = lambda **kwargs: doctor.Report(
                schema=1,
                status="partial",
                wrapper=doctor.WrapperInfo(None, None, "none"),
                linker=doctor.LinkerInfo("none", None, None, "none"),
                sccache=doctor.SccacheInfo("missing", None, None, None, None),
                budgets=doctor.BudgetsInfo(None, "default", None, "none"),
                diagnostics=[],
            )
            exit_code = doctor.main(["--require-acceleration"])
        finally:
            doctor.build_report = real_build_report
        self.assertEqual(exit_code, doctor.EXIT_STRICT_UNMET)

    def test_error_status_exits_70_even_with_strict(self) -> None:
        real_build_report = doctor.build_report
        try:
            doctor.build_report = lambda **kwargs: doctor.Report(
                schema=1,
                status="error",
                wrapper=doctor.WrapperInfo(None, None, "none"),
                linker=doctor.LinkerInfo("none", None, None, "none"),
                sccache=doctor.SccacheInfo("missing", None, None, None, None),
                budgets=doctor.BudgetsInfo(None, "default", None, "none"),
                diagnostics=[doctor.DIAG_INVALID_CONFIG],
            )
            exit_code = doctor.main([])
        finally:
            doctor.build_report = real_build_report
        self.assertEqual(exit_code, doctor.EXIT_ERROR)


class TimeoutBehaviorTest(unittest.TestCase):
    def test_real_subprocess_timeout_is_treated_as_wedged(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            slow_bin = Path(tmp) / "sccache"
            make_executable(slow_bin, "#!/bin/sh\nsleep 5\n")

            start = time.monotonic()
            result = doctor.run_sccache_show_stats(str(slow_bin), timeout=0.2)
            elapsed = time.monotonic() - start

        self.assertTrue(result.timed_out)
        self.assertFalse(result.ok)
        self.assertLess(elapsed, 4.0)


class MutationServiceSecretGuardTest(unittest.TestCase):
    def test_only_read_only_show_stats_invocation_used(self) -> None:
        calls = []

        def fake_run(cmd, **kwargs):
            calls.append(cmd)

            class _Proc:
                returncode = 0
                stdout = "{}"

            return _Proc()

        original = doctor.subprocess.run
        doctor.subprocess.run = fake_run
        try:
            doctor.run_sccache_show_stats("/usr/bin/sccache", timeout=1.0)
        finally:
            doctor.subprocess.run = original

        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0], ["/usr/bin/sccache", "--show-stats", "--stats-format", "json"])

    def test_source_never_invokes_mutating_or_service_commands(self) -> None:
        source = Path(doctor.__file__).read_text(encoding="utf-8")
        forbidden = [
            "--stop-server",
            "--start-server",
            "pip install",
            "apt-get",
            "apt install",
            "sccache install",
            " rm ",
            "shutil.rmtree",
            "os.remove",
            "os.unlink",
        ]
        for token in forbidden:
            self.assertNotIn(token, source, f"forbidden token found in doctor source: {token!r}")

    def test_stats_output_is_not_echoed_with_environment_secrets(self) -> None:
        report = doctor.Report(
            schema=1,
            status="inactive",
            wrapper=doctor.WrapperInfo(None, None, "none"),
            linker=doctor.LinkerInfo("none", None, None, "none"),
            sccache=doctor.SccacheInfo("missing", None, None, None, None),
            budgets=doctor.BudgetsInfo(None, "default", None, "none"),
            diagnostics=[],
        )
        rendered = doctor.render_human(report)
        as_json = json.dumps(asdict(report))
        for banned in ("AWS_SECRET", "API_KEY", "TOKEN="):
            self.assertNotIn(banned, rendered)
            self.assertNotIn(banned, as_json)


if __name__ == "__main__":
    unittest.main()
