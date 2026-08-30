#!/usr/bin/env python3
"""Select the smallest safe Shatter gate set for a Git diff."""

from __future__ import annotations

import argparse
from pathlib import Path, PurePosixPath
import subprocess
import sys
from typing import Iterable


GATE_ORDER = (
    "docs",
    "meta",
    "schemas",
    "smoke",
    "core:clippy",
    "core:test",
    "cli:clippy",
    "cli:test",
    "ts:typecheck",
    "ts:test",
    "go:vet",
    "go:test",
    "rust-fe:clippy",
    "rust-fe:test",
    "rust-rt:clippy",
    "rust-rt:test",
    "e2e-ts",
    "e2e-go",
    "e2e-rust",
    "parity",
    "conformance",
    "walkthrough",
    "gauntlet",
    "check",
)

ALL_E2E = {"e2e-ts", "e2e-go", "e2e-rust"}
CODE_GATES = {
    "core:clippy",
    "core:test",
    "cli:clippy",
    "cli:test",
    "ts:typecheck",
    "ts:test",
    "go:vet",
    "go:test",
    "rust-fe:clippy",
    "rust-fe:test",
    "rust-rt:clippy",
    "rust-rt:test",
    *ALL_E2E,
    "check",
}
CHECK_COVERED_GATES = {
    "docs",
    "meta",
    "schemas",
    "core:clippy",
    "core:test",
    "cli:clippy",
    "cli:test",
    "ts:typecheck",
    "ts:test",
    "go:vet",
    "go:test",
    "rust-fe:clippy",
    "rust-fe:test",
    "rust-rt:clippy",
    "rust-rt:test",
    *ALL_E2E,
    "parity",
    "conformance",
}
META_SCRIPTS = {
    "scripts/drift-patrol.py",
    "scripts/test_drift_patrol.py",
    "scripts/test_release_workflow.py",
    "scripts/test_install_manifest.py",
    "scripts/package_npm_release.py",
    "scripts/test_package_npm_release.py",
    "scripts/retention_continuous_releases.py",
    "scripts/test_retention_continuous_releases.py",
    "scripts/cleanup-merged-remote-branches.sh",
    "scripts/test_cleanup_merged_remote_branches.sh",
    "scripts/setup-hooks.sh",
    "scripts/test_setup_hooks.sh",
}
CORE_PIPELINE_FILES = {
    "pipeline.rs",
    "planner_consumer.rs",
    "protocol.rs",
    "solver.rs",
    "strategy.rs",
    "sym_expr.rs",
}
CLI_PIPELINE_PREFIXES = (
    "shatter-cli/src/commands/",
)
CLI_PIPELINE_FILES = {
    "shatter-cli/src/args.rs",
    "shatter-cli/src/main.rs",
}
CLI_GAUNTLET_FILES = {
    *CLI_PIPELINE_FILES,
    "shatter-cli/src/render.rs",
}


def _is_core_pipeline(path: str) -> bool:
    relative = PurePosixPath(path).relative_to("shatter-core/src")
    name = relative.name
    relative_text = relative.as_posix()
    return (
        name in CORE_PIPELINE_FILES
        or "explorer" in relative_text
        or "orchestrator" in relative_text
        or any(part.startswith("instrument") for part in relative.parts)
    )


def _classify(path: str) -> set[str] | None:
    if path.startswith((".beads/", ".claude/")):
        return set()
    if path.endswith(".md") or path.startswith("docs/"):
        return {"docs"}
    if PurePosixPath(path).name in {"Taskfile.yml", "Cargo.toml", "Cargo.lock"}:
        return None
    if (
        path.startswith((".github/workflows/", ".semgrep/", "shatter-go-tool/"))
        or path == "install.sh"
        or path in META_SCRIPTS
    ):
        return {"meta"}
    if path.startswith("protocol/"):
        return {
            "schemas",
            "parity",
            "conformance",
            "ts:test",
            "go:test",
            "rust-fe:test",
        }
    if path.startswith("shatter-core/"):
        gates = {"core:test", "core:clippy"}
        if path.startswith("shatter-core/src/") and _is_core_pipeline(path):
            gates.update(ALL_E2E)
        e2e_files = {
            "shatter-core/tests/e2e_concolic.rs": "e2e-ts",
            "shatter-core/tests/e2e_concolic_go.rs": "e2e-go",
            "shatter-core/tests/e2e_concolic_rust.rs": "e2e-rust",
        }
        if path in e2e_files:
            gates.add(e2e_files[path])
        return gates
    if path.startswith("shatter-cli/"):
        gates = {"cli:test", "cli:clippy"}
        if path in CLI_PIPELINE_FILES or path.startswith(CLI_PIPELINE_PREFIXES):
            gates.update(ALL_E2E)
            gates.add("gauntlet")
        elif path in CLI_GAUNTLET_FILES or path.startswith("shatter-cli/templates/"):
            gates.add("gauntlet")
        return gates
    if path.startswith("shatter-ts/"):
        return {"ts:test", "ts:typecheck", "e2e-ts", "parity", "conformance"}
    if path.startswith("shatter-go/"):
        return {"go:test", "go:vet", "e2e-go", "parity", "conformance"}
    if path.startswith("shatter-rust-runtime/"):
        return {"rust-rt:test", "rust-rt:clippy", "e2e-rust"}
    if path.startswith("shatter-rust/"):
        return {"rust-fe:test", "rust-fe:clippy", "e2e-rust", "parity", "conformance"}
    if path.startswith("demo/walkthrough"):
        return {"walkthrough"}
    if (
        path.startswith(("demo/gauntlet", "demo/fixtures/"))
        or path == "demo/test_gauntlet_check_output.py"
    ):
        return {"gauntlet"}
    if path == "benchmarks/sample-manifest.json":
        return {"walkthrough", "gauntlet"}
    if path.startswith("examples/") or path == "scripts/examples_checkout.py":
        return {"smoke", *ALL_E2E}
    return None


def select_gates(paths: Iterable[str]) -> list[str]:
    selected: set[str] = set()
    unmatched = False
    for raw_path in paths:
        path = raw_path.strip().replace("\\", "/")
        if not path:
            continue
        gates = _classify(path)
        if gates is None:
            unmatched = True
        else:
            selected.update(gates)

    if unmatched:
        selected.difference_update(CHECK_COVERED_GATES)
        selected.add("check")
    if selected.intersection(CODE_GATES):
        selected.add("smoke")
    return [gate for gate in GATE_ORDER if gate in selected]


def _git(args: list[str], cwd: Path) -> str:
    completed = subprocess.run(
        ["git", *args],
        cwd=cwd,
        text=True,
        capture_output=True,
    )
    if completed.returncode:
        detail = completed.stderr.strip() or completed.stdout.strip()
        raise RuntimeError(f"git {' '.join(args)} failed: {detail}")
    return completed.stdout.strip()


def changed_paths(base: str, head: str, cwd: Path = Path.cwd()) -> list[str]:
    merge_base = _git(["merge-base", base, head], cwd)
    output = _git(["diff", "--name-only", f"{merge_base}..{head}", "--"], cwd)
    return output.splitlines() if output else []


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", default="origin/main", help="base revision (default: origin/main)")
    parser.add_argument(
        "--head",
        action="append",
        dest="heads",
        help="head revision to inspect; repeat to union pushed heads (default: HEAD)",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    try:
        paths = {
            path
            for head in (args.heads or ["HEAD"])
            for path in changed_paths(args.base, head)
        }
    except RuntimeError as error:
        print(f"affected-gates: {error}", file=sys.stderr)
        return 2
    gates = select_gates(sorted(paths))
    if gates:
        print("\n".join(gates))
    else:
        print("(none)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
