#!/usr/bin/env python3
"""Initialize Shatter targets and add simple wrapper commands."""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import shutil
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
DISCOVER_PATH = SCRIPT_DIR / "discover_targets.py"
DISCOVER_SPEC = importlib.util.spec_from_file_location("discover_targets", DISCOVER_PATH)
assert DISCOVER_SPEC is not None and DISCOVER_SPEC.loader is not None
DISCOVER_MODULE = importlib.util.module_from_spec(DISCOVER_SPEC)
sys.modules[DISCOVER_SPEC.name] = DISCOVER_MODULE
DISCOVER_SPEC.loader.exec_module(DISCOVER_MODULE)

SUPPORTED_SURFACES = {"package-json-scripts", "taskfile"}
DEFAULT_WRAPPER = "shatter scan ."
DEFAULT_WRAPPER_NAME = "shatter"
DEFAULT_TASK_DESC = "Run Shatter broad analysis"


@dataclass(frozen=True)
class IntegrationDecision:
    status: str
    reason: str
    selected_surface: dict[str, object] | None = None


def supported_surfaces(target: dict[str, object]) -> tuple[list[dict[str, object]], list[dict[str, object]]]:
    surfaces = target.get("surfaces", [])
    local_supported = [
        surface
        for surface in surfaces
        if surface["scope"] == "local" and surface["type"] in SUPPORTED_SURFACES
    ]
    ancestor_supported = [
        surface
        for surface in surfaces
        if surface["scope"] == "ancestor" and surface["type"] in SUPPORTED_SURFACES
    ]
    return local_supported, ancestor_supported


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Initialize Shatter and add wrapper commands for simple targets.",
    )
    parser.add_argument("--root", default=".", help="Repository root to inspect.")
    parser.add_argument("--apply", action="store_true", help="Apply changes in place.")
    parser.add_argument("--json", action="store_true", help="Print JSON instead of text.")
    parser.add_argument(
        "--skip-init",
        action="store_true",
        help="Do not run `shatter init`; useful for dry runs and tests.",
    )
    parser.add_argument(
        "--force-over-four",
        action="store_true",
        help="Allow apply mode even when more than four targets are detected.",
    )
    parser.add_argument(
        "--shatter-command",
        default="shatter",
        help="Shatter executable to invoke for init (default: shatter).",
    )
    return parser.parse_args()


def docs_preference(report: dict[str, object]) -> str | None:
    hints = report.get("doc_hints", [])
    if not hints:
        return None
    top = hints[0]
    return top["type"] if isinstance(top, dict) else None


def choose_surface(target: dict[str, object], preferred_type: str | None) -> IntegrationDecision:
    local_supported, ancestor_supported = supported_surfaces(target)

    if len(local_supported) == 1:
        return IntegrationDecision(
            status="integrated",
            reason="single local supported surface",
            selected_surface=local_supported[0],
        )

    if len(local_supported) > 1 and preferred_type is not None:
        preferred = [surface for surface in local_supported if surface["type"] == preferred_type]
        if len(preferred) == 1:
            return IntegrationDecision(
                status="integrated",
                reason=f"preferred by docs hint: {preferred_type}",
                selected_surface=preferred[0],
            )

    if local_supported:
        return IntegrationDecision(
            status="ambiguous",
            reason="multiple supported local surfaces",
        )

    if ancestor_supported:
        return IntegrationDecision(
            status="ambiguous",
            reason="only ancestor surfaces available",
        )

    return IntegrationDecision(
        status="skipped",
        reason="no supported local surface",
    )


def ensure_shatter_available(command: str) -> None:
    if shutil.which(command) is None:
        raise RuntimeError(f"{command!r} not found on PATH")


def run_shatter_init(command: str, repo_root: Path, target_root: str) -> None:
    target_path = repo_root if target_root == "." else repo_root / target_root
    subprocess.run(
        [command, "init", "--directory", str(target_path)],
        cwd=repo_root,
        check=True,
    )


def wrapper_command_for_surface(repo_root: Path, target_root: str, surface: dict[str, object]) -> str:
    target_path = repo_root if target_root == "." else repo_root / target_root
    surface_dir = (repo_root / surface["path"]).parent
    relative_target = os.path.relpath(target_path, surface_dir).replace(os.sep, "/")
    scan_root = "." if relative_target == "." else relative_target
    return f"shatter scan {scan_root}"


def proposal_edit_for_surface(
    repo_root: Path,
    target_root: str,
    surface: dict[str, object],
) -> dict[str, object]:
    wrapper_command = wrapper_command_for_surface(repo_root, target_root, surface)
    if surface["type"] == "package-json-scripts":
        return {
            "kind": "package-json-script",
            "path": surface["path"],
            "script_name": DEFAULT_WRAPPER_NAME,
            "script_value": wrapper_command,
        }
    if surface["type"] == "taskfile":
        return {
            "kind": "taskfile-task",
            "path": surface["path"],
            "task_name": DEFAULT_WRAPPER_NAME,
            "task_desc": DEFAULT_TASK_DESC,
            "cmds": [wrapper_command],
        }
    raise RuntimeError(f"unsupported surface type: {surface['type']}")


def proposal_for_surface(
    repo_root: Path,
    target: dict[str, object],
    surface: dict[str, object],
    rationale: str,
) -> dict[str, object]:
    return {
        "proposal_type": "wrapper-suggestion",
        "target_root": target["root"],
        "surface": surface,
        "wrapper_name": DEFAULT_WRAPPER_NAME,
        "wrapper_command": wrapper_command_for_surface(repo_root, target["root"], surface),
        "edit": proposal_edit_for_surface(repo_root, target["root"], surface),
        "rationale": rationale,
    }


def proposals_for_target(
    repo_root: Path,
    target: dict[str, object],
    decision: IntegrationDecision,
) -> list[dict[str, object]]:
    local_supported, ancestor_supported = supported_surfaces(target)
    if decision.status != "ambiguous":
        return []
    if decision.reason == "multiple supported local surfaces":
        return [
            proposal_for_surface(
                repo_root,
                target,
                surface,
                "candidate supported local surface; manual choice required",
            )
            for surface in local_supported
        ]
    if decision.reason == "only ancestor surfaces available":
        return [
            proposal_for_surface(
                repo_root,
                target,
                surface,
                "candidate ancestor surface; review target scoping before editing",
            )
            for surface in ancestor_supported
        ]
    return []


def write_package_json(path: Path, wrapper_command: str) -> bool:
    data = DISCOVER_MODULE.load_package_json(path)
    if not isinstance(data, dict):
        raise RuntimeError(f"failed to parse {path}")
    scripts = data.setdefault("scripts", {})
    if not isinstance(scripts, dict):
        raise RuntimeError(f"{path} has non-object scripts field")
    current = scripts.get(DEFAULT_WRAPPER_NAME)
    if current == wrapper_command:
        return False
    if current is not None and current != wrapper_command:
        raise RuntimeError(f"{path} already defines scripts.{DEFAULT_WRAPPER_NAME}")
    scripts[DEFAULT_WRAPPER_NAME] = wrapper_command
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")
    return True


def taskfile_has_shatter_task(content: str) -> bool:
    return "\n  shatter:\n" in f"\n{content}"


def write_taskfile(path: Path, wrapper_command: str) -> bool:
    content = path.read_text(encoding="utf-8")
    if taskfile_has_shatter_task(content):
        return False

    addition = (
        f"\n  {DEFAULT_WRAPPER_NAME}:\n"
        f"    desc: {DEFAULT_TASK_DESC}\n"
        "    cmds:\n"
        f"      - {wrapper_command}\n"
    )

    if "\ntasks:\n" in content:
        new_content = content.rstrip() + addition
    else:
        new_content = content.rstrip() + "\n\ntasks:\n" + addition.lstrip("\n")
    path.write_text(new_content + "\n", encoding="utf-8")
    return True


def apply_surface(repo_root: Path, target_root: str, surface: dict[str, object]) -> bool:
    surface_path = repo_root / surface["path"]
    surface_type = surface["type"]
    wrapper_command = wrapper_command_for_surface(repo_root, target_root, surface)
    if surface_type == "package-json-scripts":
        return write_package_json(surface_path, wrapper_command)
    if surface_type == "taskfile":
        return write_taskfile(surface_path, wrapper_command)
    raise RuntimeError(f"unsupported surface type: {surface_type}")


def build_results(report: dict[str, object], apply: bool, skip_init: bool, command: str) -> dict[str, object]:
    preferred = docs_preference(report)
    results: list[dict[str, object]] = []

    if apply and report["requires_confirmation"]:
        return {
            "repo_root": report["repo_root"],
            "target_count": report["target_count"],
            "requires_confirmation": True,
            "applied": False,
            "proposal_count": 0,
            "targets": [],
            "error": "more than four targets detected; confirmation required",
        }

    if apply and not skip_init:
        ensure_shatter_available(command)

    repo_root = Path(report["repo_root"])
    for target in report["targets"]:
        decision = choose_surface(target, preferred)
        entry = {
            "root": target["root"],
            "languages": target["languages"],
            "status": decision.status,
            "reason": decision.reason,
            "selected_surface": decision.selected_surface,
            "proposals": proposals_for_target(repo_root, target, decision),
            "init_ran": False,
            "wrapper_updated": False,
        }

        if apply and decision.status == "integrated" and decision.selected_surface is not None:
            if not skip_init:
                run_shatter_init(command, repo_root, target["root"])
                entry["init_ran"] = True
            entry["wrapper_updated"] = apply_surface(
                repo_root,
                target["root"],
                decision.selected_surface,
            )

        results.append(entry)

    return {
        "repo_root": report["repo_root"],
        "target_count": report["target_count"],
        "requires_confirmation": report["requires_confirmation"],
        "applied": apply,
        "preferred_surface": preferred,
        "proposal_count": sum(len(entry["proposals"]) for entry in results),
        "targets": results,
    }


def print_human_summary(results: dict[str, object]) -> None:
    print(f"Repository: {results['repo_root']}")
    print(f"Targets: {results['target_count']}")
    print(f"Preferred surface: {results.get('preferred_surface') or 'none'}")
    print(f"Proposals: {results.get('proposal_count', 0)}")
    if results.get("error"):
        print(f"Error: {results['error']}")
        return

    for target in results["targets"]:
        print()
        print(f"- {target['root']}: {target['status']} ({target['reason']})")
        surface = target.get("selected_surface")
        if surface:
            print(f"  surface: {surface['type']} at {surface['path']}")
        proposals = target.get("proposals", [])
        for proposal in proposals:
            print(
                "  proposal:"
                f" {proposal['surface']['type']} at {proposal['edit']['path']}"
                f" -> {proposal['wrapper_command']}"
            )
        if results["applied"]:
            print(f"  init ran: {'yes' if target['init_ran'] else 'no'}")
            print(f"  wrapper updated: {'yes' if target['wrapper_updated'] else 'no'}")


def main() -> int:
    args = parse_args()
    report = DISCOVER_MODULE.build_report(Path(args.root).resolve())
    if args.apply and report["requires_confirmation"] and not args.force_over_four:
        results = {
            "repo_root": report["repo_root"],
            "target_count": report["target_count"],
            "requires_confirmation": True,
            "applied": False,
            "preferred_surface": docs_preference(report),
            "proposal_count": 0,
            "targets": [],
            "error": "more than four targets detected; confirmation required",
        }
    else:
        results = build_results(report, args.apply, args.skip_init, args.shatter_command)

    if args.json:
        print(json.dumps(results, indent=2, sort_keys=True))
    else:
        print_human_summary(results)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
