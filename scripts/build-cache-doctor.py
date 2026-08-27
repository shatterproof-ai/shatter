#!/usr/bin/env python3
"""Diagnose build acceleration (sccache wrapper + linker) configuration."""
from __future__ import annotations

import argparse
import json
import os
import re
import stat
import subprocess
import sys
import tomllib
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Callable, Sequence

SCHEMA_VERSION = 1
TARGET_TRIPLE = "x86_64-unknown-linux-gnu"
TARGET_RUSTFLAGS_ENV = "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUSTFLAGS"
SCCACHE_STATS_TIMEOUT_SECONDS = 2.0

DIAG_WRAPPER_MISSING = "wrapper_missing"
DIAG_WRAPPER_NOT_SCCACHE = "wrapper_not_sccache"
DIAG_WRAPPER_UNEXECUTABLE = "wrapper_unexecutable"
DIAG_LINKER_MISSING = "linker_missing"
DIAG_LINKER_UNEXECUTABLE = "linker_unexecutable"
DIAG_SCCACHE_MISSING = "sccache_missing"
DIAG_SCCACHE_WEDGED = "sccache_wedged"
DIAG_SCCACHE_STATS_ERROR = "sccache_stats_error"
DIAG_INVALID_CONFIG = "invalid_config"

DIAGNOSTIC_ORDER = [
    DIAG_WRAPPER_MISSING,
    DIAG_WRAPPER_NOT_SCCACHE,
    DIAG_WRAPPER_UNEXECUTABLE,
    DIAG_LINKER_MISSING,
    DIAG_LINKER_UNEXECUTABLE,
    DIAG_SCCACHE_MISSING,
    DIAG_SCCACHE_WEDGED,
    DIAG_SCCACHE_STATS_ERROR,
    DIAG_INVALID_CONFIG,
]

EXIT_OK = 0
EXIT_STRICT_UNMET = 2
EXIT_ERROR = 70
EXIT_ARGS = 64

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parent

LINKER_DISCOVERY_ORDER = (("mold", ("mold",)), ("lld", ("ld.lld", "lld")))


@dataclass(frozen=True)
class WrapperInfo:
    configured: str | None
    resolved_path: str | None
    source: str


@dataclass(frozen=True)
class LinkerInfo:
    name: str
    configured: str | None
    resolved_path: str | None
    source: str


@dataclass(frozen=True)
class SccacheInfo:
    state: str
    binary_path: str | None
    cache_dir: str | None
    cache_size_bytes: int | None
    stats: dict | None


@dataclass(frozen=True)
class BudgetsInfo:
    cargo_build_jobs: str | None
    cargo_source: str
    shatter_cpu_budget: str | None
    shatter_source: str


@dataclass(frozen=True)
class Report:
    schema: int
    status: str
    wrapper: WrapperInfo
    linker: LinkerInfo
    sccache: SccacheInfo
    budgets: BudgetsInfo
    diagnostics: list[str] = field(default_factory=list)


@dataclass(frozen=True)
class StatsResult:
    ok: bool
    timed_out: bool
    stats: dict | None
    cache_dir: str | None


def load_toml(path: Path | None) -> tuple[dict | None, bool]:
    if path is None or not path.is_file():
        return None, False
    try:
        with path.open("rb") as handle:
            return tomllib.load(handle), False
    except (tomllib.TOMLDecodeError, OSError):
        return None, True


def safe_table(container: object, key: str) -> dict:
    """Return container[key] if it is a table, else {} -- never raises on structurally-wrong TOML."""
    if not isinstance(container, dict):
        return {}
    value = container.get(key)
    return value if isinstance(value, dict) else {}


def config_structure_is_valid(cfg: dict | None) -> bool:
    """Reject configs where 'build' or 'target'/'target.<triple>' exist but aren't tables."""
    if not cfg:
        return True
    build = cfg.get("build")
    if build is not None and not isinstance(build, dict):
        return False
    target = cfg.get("target")
    if target is not None:
        if not isinstance(target, dict):
            return False
        triple = target.get(TARGET_TRIPLE)
        if triple is not None and not isinstance(triple, dict):
            return False
    return True


def find_executable(names: Sequence[str], path_dirs: Sequence[str]) -> Path | None:
    for directory in path_dirs:
        for name in names:
            candidate = Path(directory) / name
            if candidate.is_file():
                return candidate
    return None


def extract_fuse_ld(value: object) -> str | None:
    if isinstance(value, list):
        joined = " ".join(str(item) for item in value)
    else:
        joined = str(value)
    match = re.search(r"-fuse-ld=(\S+)", joined)
    return match.group(1) if match else None


def get_target_rustflags(cfg: dict | None) -> object | None:
    if not cfg:
        return None
    return safe_table(safe_table(cfg, "target"), TARGET_TRIPLE).get("rustflags")


def resolve_wrapper(
    env: dict[str, str],
    user_cfg: dict | None,
    repo_cfg: dict | None,
    path_dirs: Sequence[str],
) -> tuple[WrapperInfo, str]:
    configured: str | None = None
    source = "none"
    user_wrapper = safe_table(user_cfg, "build").get("rustc-wrapper")
    repo_wrapper = safe_table(repo_cfg, "build").get("rustc-wrapper")
    if env.get("RUSTC_WRAPPER"):
        configured, source = env["RUSTC_WRAPPER"], "environment"
    elif isinstance(user_wrapper, str) and user_wrapper:
        configured, source = user_wrapper, "user_config"
    elif isinstance(repo_wrapper, str) and repo_wrapper:
        configured, source = repo_wrapper, "repo_config"

    if configured is None:
        return WrapperInfo(configured=None, resolved_path=None, source="none"), "ok"

    expanded = Path(configured).expanduser()
    if expanded.is_absolute():
        candidate = expanded if expanded.is_file() else None
    else:
        candidate = find_executable([configured], path_dirs)

    if candidate is None:
        return WrapperInfo(configured=configured, resolved_path=None, source=source), "missing"
    if not os.access(candidate, os.X_OK):
        return WrapperInfo(configured=configured, resolved_path=str(candidate), source=source), "unexecutable"
    return WrapperInfo(configured=configured, resolved_path=str(candidate), source=source), "ok"


def normalize_linker_name(fuse_ld_value: str) -> str:
    if fuse_ld_value == "mold":
        return "mold"
    if fuse_ld_value == "lld":
        return "lld"
    return "other"


def linker_candidate_names(fuse_ld_value: str) -> tuple[str, ...]:
    if fuse_ld_value == "lld":
        return ("ld.lld", "lld")
    return (fuse_ld_value, f"ld.{fuse_ld_value}")


def resolve_linker(
    env: dict[str, str],
    user_cfg: dict | None,
    repo_cfg: dict | None,
    path_dirs: Sequence[str],
) -> tuple[LinkerInfo, str]:
    fuse_ld_value: str | None = None
    source = "none"

    env_flags = env.get(TARGET_RUSTFLAGS_ENV)
    if env_flags:
        value = extract_fuse_ld(env_flags)
        if value:
            fuse_ld_value, source = value, "environment"

    if fuse_ld_value is None:
        value = extract_fuse_ld(get_target_rustflags(user_cfg) or "")
        if value:
            fuse_ld_value, source = value, "user_config"

    if fuse_ld_value is None:
        value = extract_fuse_ld(get_target_rustflags(repo_cfg) or "")
        if value:
            fuse_ld_value, source = value, "repo_config"

    if fuse_ld_value is not None:
        name = normalize_linker_name(fuse_ld_value)
        found = find_executable(linker_candidate_names(fuse_ld_value), path_dirs)
        if found is None:
            return LinkerInfo(name=name, configured=fuse_ld_value, resolved_path=None, source=source), "missing"
        if not os.access(found, os.X_OK):
            return LinkerInfo(name=name, configured=fuse_ld_value, resolved_path=str(found), source=source), "unexecutable"
        return LinkerInfo(name=name, configured=fuse_ld_value, resolved_path=str(found), source=source), "ok"

    for name, candidates in LINKER_DISCOVERY_ORDER:
        found = find_executable(candidates, path_dirs)
        if found is not None and os.access(found, os.X_OK):
            return LinkerInfo(name=name, configured=None, resolved_path=str(found), source="none"), "ok"

    return LinkerInfo(name="none", configured=None, resolved_path=None, source="none"), "ok"


def run_sccache_show_stats(binary_path: str, timeout: float = SCCACHE_STATS_TIMEOUT_SECONDS) -> StatsResult:
    try:
        proc = subprocess.run(
            [binary_path, "--show-stats", "--stats-format", "json"],
            capture_output=True,
            text=True,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired:
        return StatsResult(ok=False, timed_out=True, stats=None, cache_dir=None)
    except OSError:
        return StatsResult(ok=False, timed_out=False, stats=None, cache_dir=None)

    if proc.returncode != 0:
        return StatsResult(ok=False, timed_out=False, stats=None, cache_dir=None)

    try:
        data = json.loads(proc.stdout)
    except json.JSONDecodeError:
        return StatsResult(ok=False, timed_out=False, stats=None, cache_dir=None)

    cache_dir = None
    if isinstance(data, dict):
        for key in ("cache_dir", "cache_location", "CacheDir"):
            value = data.get(key)
            if isinstance(value, str) and value:
                cache_dir = extract_local_disk_path(value)
                if cache_dir:
                    break
    return StatsResult(ok=True, timed_out=False, stats=data, cache_dir=cache_dir)


def extract_local_disk_path(cache_location: str) -> str | None:
    match = re.match(r'Local disk: "(.+)"$', cache_location)
    if match:
        return match.group(1)
    if cache_location.startswith(("/", "~")):
        return cache_location
    return None


def resolve_cache_dir(env: dict[str, str], home: Path, stats_result: StatsResult | None) -> str:
    if env.get("SCCACHE_DIR"):
        return env["SCCACHE_DIR"]
    if stats_result is not None and stats_result.cache_dir:
        return stats_result.cache_dir
    xdg = env.get("XDG_CACHE_HOME") or str(home / ".cache")
    return str(Path(xdg) / "sccache")


def compute_cache_size(cache_dir: str) -> int | None:
    path = Path(cache_dir)
    if not path.is_dir():
        return None

    total = 0
    unreadable = False

    def on_walk_error(_exc: OSError) -> None:
        nonlocal unreadable
        unreadable = True

    try:
        for root, _dirs, files in os.walk(path, onerror=on_walk_error, followlinks=False):
            for name in files:
                file_path = Path(root) / name
                try:
                    file_stat = file_path.lstat()
                except OSError:
                    unreadable = True
                    continue
                if stat.S_ISREG(file_stat.st_mode):
                    total += file_stat.st_size
    except OSError:
        return None

    if unreadable:
        return None
    return total


def resolve_sccache(
    env: dict[str, str],
    home: Path,
    wrapper: WrapperInfo,
    wrapper_exec_status: str,
    path_dirs: Sequence[str],
    run_stats: Callable[[str, float], StatsResult],
) -> tuple[SccacheInfo, list[str]]:
    effective_is_sccache = (
        wrapper.source != "none"
        and wrapper_exec_status == "ok"
        and wrapper.resolved_path is not None
        and Path(wrapper.resolved_path).name == "sccache"
    )

    binary_path: str | None = None
    if effective_is_sccache:
        binary_path = wrapper.resolved_path
    else:
        found = find_executable(["sccache"], path_dirs)
        if found is not None and os.access(found, os.X_OK):
            binary_path = str(found)

    if binary_path is None:
        cache_dir = resolve_cache_dir(env, home, None)
        return (
            SccacheInfo(
                state="missing",
                binary_path=None,
                cache_dir=cache_dir,
                cache_size_bytes=compute_cache_size(cache_dir),
                stats=None,
            ),
            [DIAG_SCCACHE_MISSING],
        )

    result = run_stats(binary_path, SCCACHE_STATS_TIMEOUT_SECONDS)
    diagnostics: list[str] = []
    if result.timed_out:
        state = "wedged"
        diagnostics.append(DIAG_SCCACHE_WEDGED)
    elif not result.ok:
        state = "error"
        diagnostics.append(DIAG_SCCACHE_STATS_ERROR)
    else:
        state = "active" if effective_is_sccache else "installed_inactive"

    cache_dir = resolve_cache_dir(env, home, result if result.ok else None)
    sccache_info = SccacheInfo(
        state=state,
        binary_path=binary_path,
        cache_dir=cache_dir,
        cache_size_bytes=compute_cache_size(cache_dir),
        stats=result.stats,
    )
    return sccache_info, diagnostics


def resolve_cargo_budget(env: dict[str, str], repo_cfg: dict | None) -> tuple[str | None, str]:
    if env.get("CARGO_BUILD_JOBS"):
        return env["CARGO_BUILD_JOBS"], "gate_environment"
    jobs = safe_table(repo_cfg, "build").get("jobs")
    if jobs is not None:
        return str(jobs), "repo_config"
    return None, "default"


def resolve_shatter_budget(env: dict[str, str]) -> tuple[str | None, str]:
    if env.get("SHATTER_CPU_BUDGET"):
        return env["SHATTER_CPU_BUDGET"], "gate_environment"
    return None, "none"


def build_report(
    *,
    env: dict[str, str],
    home: Path,
    cargo_home: Path | None,
    repo_root: Path,
    path_dirs: Sequence[str],
    run_stats: Callable[[str, float], StatsResult] = run_sccache_show_stats,
) -> Report:
    user_cfg_path = (cargo_home / "config.toml") if cargo_home else (home / ".cargo" / "config.toml")
    repo_cfg_path = repo_root / ".cargo" / "config.toml"

    user_cfg, user_cfg_err = load_toml(user_cfg_path)
    repo_cfg, repo_cfg_err = load_toml(repo_cfg_path)
    user_cfg_err = user_cfg_err or not config_structure_is_valid(user_cfg)
    repo_cfg_err = repo_cfg_err or not config_structure_is_valid(repo_cfg)
    config_error = user_cfg_err or repo_cfg_err

    wrapper, wrapper_exec = resolve_wrapper(env, user_cfg, repo_cfg, path_dirs)
    linker, linker_exec = resolve_linker(env, user_cfg, repo_cfg, path_dirs)
    sccache_info, sccache_diags = resolve_sccache(env, home, wrapper, wrapper_exec, path_dirs, run_stats)

    diagnostics: list[str] = []
    if wrapper.source != "none":
        if wrapper_exec == "missing":
            diagnostics.append(DIAG_WRAPPER_MISSING)
        elif wrapper_exec == "unexecutable":
            diagnostics.append(DIAG_WRAPPER_UNEXECUTABLE)
        elif wrapper_exec == "ok" and wrapper.resolved_path and Path(wrapper.resolved_path).name != "sccache":
            diagnostics.append(DIAG_WRAPPER_NOT_SCCACHE)
    if linker.source != "none":
        if linker_exec == "missing":
            diagnostics.append(DIAG_LINKER_MISSING)
        elif linker_exec == "unexecutable":
            diagnostics.append(DIAG_LINKER_UNEXECUTABLE)
    diagnostics.extend(sccache_diags)
    if config_error:
        diagnostics.append(DIAG_INVALID_CONFIG)

    seen = set(diagnostics)
    diagnostics = [d for d in DIAGNOSTIC_ORDER if d in seen]

    cargo_jobs, cargo_source = resolve_cargo_budget(env, repo_cfg)
    shatter_budget, shatter_source = resolve_shatter_budget(env)
    budgets = BudgetsInfo(
        cargo_build_jobs=cargo_jobs,
        cargo_source=cargo_source,
        shatter_cpu_budget=shatter_budget,
        shatter_source=shatter_source,
    )

    linker_executable = linker.resolved_path is not None and linker_exec == "ok"
    accelerated = (
        sccache_info.state == "active"
        and linker.source != "none"
        and linker_executable
        and linker.name in ("mold", "lld")
    )

    if config_error or sccache_info.state == "error":
        status = "error"
    elif accelerated:
        status = "accelerated"
    elif sccache_info.state != "missing" or linker.source != "none" or linker.name != "none":
        status = "partial"
    else:
        status = "inactive"

    return Report(
        schema=SCHEMA_VERSION,
        status=status,
        wrapper=wrapper,
        linker=linker,
        sccache=sccache_info,
        budgets=budgets,
        diagnostics=diagnostics,
    )


def render_human(report: Report) -> str:
    lines = [f"status: {report.status}"]
    lines.append(
        "wrapper: configured={configured} resolved_path={resolved_path} source={source}".format(
            **asdict(report.wrapper)
        )
    )
    lines.append(
        "linker: name={name} configured={configured} resolved_path={resolved_path} source={source}".format(
            **asdict(report.linker)
        )
    )
    sccache = report.sccache
    lines.append(
        f"sccache: state={sccache.state} binary_path={sccache.binary_path} "
        f"cache_dir={sccache.cache_dir} cache_size_bytes={sccache.cache_size_bytes}"
    )
    budgets = report.budgets
    lines.append(
        f"budgets: cargo_build_jobs={budgets.cargo_build_jobs} (source={budgets.cargo_source}) "
        f"shatter_cpu_budget={budgets.shatter_cpu_budget} (source={budgets.shatter_source})"
    )
    if report.diagnostics:
        lines.append("diagnostics: " + ", ".join(report.diagnostics))
    else:
        lines.append("diagnostics: none")
    return "\n".join(lines)


class _ArgumentParser(argparse.ArgumentParser):
    def error(self, message: str) -> None:  # noqa: D401 - argparse override
        self.print_usage(sys.stderr)
        print(f"{self.prog}: error: {message}", file=sys.stderr)
        sys.exit(EXIT_ARGS)


def build_arg_parser() -> argparse.ArgumentParser:
    parser = _ArgumentParser(
        prog="build-cache-doctor.py",
        description="Diagnose sccache wrapper and linker acceleration configuration.",
    )
    parser.add_argument("--json", action="store_true", help="emit the report as JSON")
    parser.add_argument(
        "--require-acceleration",
        action="store_true",
        help="exit non-zero unless status is 'accelerated'",
    )
    return parser


def default_path_dirs() -> list[str]:
    return os.environ.get("PATH", "").split(os.pathsep)


def main(argv: Sequence[str] | None = None) -> int:
    parser = build_arg_parser()
    args = parser.parse_args(argv)

    env = dict(os.environ)
    home = Path(env.get("HOME", str(Path.home())))
    cargo_home = Path(env["CARGO_HOME"]) if env.get("CARGO_HOME") else None

    report = build_report(
        env=env,
        home=home,
        cargo_home=cargo_home,
        repo_root=REPO_ROOT,
        path_dirs=default_path_dirs(),
    )

    if args.json:
        print(json.dumps(asdict(report), indent=2))
    else:
        print(render_human(report))

    if report.status == "error":
        return EXIT_ERROR
    if args.require_acceleration and report.status != "accelerated":
        return EXIT_STRICT_UNMET
    return EXIT_OK


if __name__ == "__main__":
    sys.exit(main())
