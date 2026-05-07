#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Sequence


DEFAULT_KAPOW_PROJECT = Path("~/project/kapow")
DEFAULT_REFUTE_CHECKOUT = Path("~/project/refute")


@dataclass(frozen=True)
class CheckResult:
    status: str
    message: str
    refute_binary: str
    install_command: str
    exit_code: int


def expand_path(path: str | Path) -> Path:
    return Path(path).expanduser().resolve()


def refute_binary(project: Path) -> Path:
    return project / ".agents" / "bin" / "refute"


def install_script(refute_checkout: Path) -> Path:
    return refute_checkout / "scripts" / "install-nightly.sh"


def install_command(project: Path, refute_checkout: Path) -> list[str]:
    return ["bash", str(install_script(refute_checkout)), "--project", str(project)]


def shell_join(args: Sequence[str]) -> str:
    return " ".join(args)


def check_install(project: Path, refute_checkout: Path) -> CheckResult:
    project = expand_path(project)
    refute_checkout = expand_path(refute_checkout)
    binary = refute_binary(project)
    command = install_command(project, refute_checkout)

    if not project.exists():
        return CheckResult(
            status="missing_project",
            message=f"Kapow project is missing: {project}",
            refute_binary=str(binary),
            install_command=shell_join(command),
            exit_code=2,
        )

    if not binary.exists():
        return CheckResult(
            status="missing_refute",
            message=(
                f"Refute is missing at {binary}. Install it with: "
                f"{shell_join(command)}"
            ),
            refute_binary=str(binary),
            install_command=shell_join(command),
            exit_code=2,
        )

    if not os.access(binary, os.X_OK):
        return CheckResult(
            status="not_executable",
            message=f"Refute exists but is not executable: {binary}",
            refute_binary=str(binary),
            install_command=shell_join(command),
            exit_code=2,
        )

    return CheckResult(
        status="ok",
        message=f"Refute is available at {binary}",
        refute_binary=str(binary),
        install_command=shell_join(command),
        exit_code=0,
    )


def run_command(args: Sequence[str], cwd: Path | None = None) -> int:
    completed = subprocess.run(args, cwd=cwd, check=False)
    return completed.returncode


def emit_check(result: CheckResult, *, json_output: bool) -> None:
    if json_output:
        print(json.dumps(asdict(result), sort_keys=True))
    else:
        print(result.message)


def require_refute(project: Path, refute_checkout: Path, *, json_output: bool) -> Path | None:
    result = check_install(project, refute_checkout)
    if result.exit_code != 0:
        emit_check(result, json_output=json_output)
        return None
    return Path(result.refute_binary)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(
        description="Agent wrapper for the project-local Refute install used during Kapow validation.",
    )
    parser.add_argument(
        "--project",
        default=str(DEFAULT_KAPOW_PROJECT),
        help="Kapow project root (default: ~/project/kapow)",
    )
    parser.add_argument(
        "--refute-checkout",
        default=str(DEFAULT_REFUTE_CHECKOUT),
        help="Refute source checkout (default: ~/project/refute)",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="emit wrapper status as JSON for check/path failures",
    )

    subcommands = parser.add_subparsers(dest="command", required=True)
    subcommands.add_parser("check", help="verify .agents/bin/refute exists and is executable")
    subcommands.add_parser("install", help="install or update Refute into the Kapow project")
    subcommands.add_parser("doctor", help="run refute doctor from the Kapow project")
    subcommands.add_parser("version", help="print the project-local Refute version")
    subcommands.add_parser("path", help="print the project-local Refute binary path")
    subcommands.add_parser("smoke", help="run check, version, and doctor")
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    project = expand_path(args.project)
    refute_checkout = expand_path(args.refute_checkout)

    if args.command == "check":
        result = check_install(project, refute_checkout)
        emit_check(result, json_output=args.json)
        return result.exit_code

    if args.command == "install":
        script = install_script(refute_checkout)
        if not script.exists():
            print(f"Refute installer is missing: {script}", file=sys.stderr)
            return 2
        return run_command(install_command(project, refute_checkout))

    binary = require_refute(project, refute_checkout, json_output=args.json)
    if binary is None:
        return 2

    if args.command == "path":
        if args.json:
            print(json.dumps({"refute_binary": str(binary)}, sort_keys=True))
        else:
            print(binary)
        return 0

    if args.command == "version":
        return run_command([str(binary), "version"], cwd=project)

    if args.command == "doctor":
        doctor_args = [str(binary), "doctor"]
        if args.json:
            doctor_args.append("--json")
        return run_command(doctor_args, cwd=project)

    if args.command == "smoke":
        version_code = run_command([str(binary), "version"], cwd=project)
        if version_code != 0:
            return version_code
        doctor_args = [str(binary), "doctor"]
        if args.json:
            doctor_args.append("--json")
        return run_command(doctor_args, cwd=project)

    raise AssertionError(f"unhandled command: {args.command}")


if __name__ == "__main__":
    raise SystemExit(main())
