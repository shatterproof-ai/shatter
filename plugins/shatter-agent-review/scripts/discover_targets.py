#!/usr/bin/env python3
"""Discover Shatter integration targets and likely command surfaces."""

from __future__ import annotations

import argparse
import json
import os
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable


SUPPORTED_MARKERS = {
    "Cargo.toml": "rust",
    "go.mod": "go",
    "package.json": "typescript",
}

SURFACE_FILES = {
    "Taskfile.yml": "taskfile",
    "Makefile": "makefile",
    "justfile": "justfile",
}

DOC_FILES = (
    "README.md",
    "CONTRIBUTING.md",
    "AGENTS.md",
    "CLAUDE.md",
    "docs/CI-INTEGRATION.md",
)

DOC_HINT_PATTERNS = {
    "taskfile": re.compile(r"\b(?:npx\s+task|taskfile)\b", re.IGNORECASE),
    "package-json-scripts": re.compile(r"\b(?:npm run|pnpm|yarn)\b", re.IGNORECASE),
    "makefile": re.compile(r"\bmake\s+[A-Za-z0-9:_-]+", re.IGNORECASE),
    "justfile": re.compile(r"\bjust\s+[A-Za-z0-9:_-]+", re.IGNORECASE),
}

EXCLUDED_DIR_NAMES = {
    ".git",
    ".hg",
    ".svn",
    ".next",
    ".nuxt",
    ".turbo",
    ".venv",
    ".yarn",
    "__pycache__",
    "__fixtures__",
    "build",
    "coverage",
    "dist",
    "fixtures",
    "node_modules",
    "out",
    "target",
    "testdata",
    "tmp",
    "vendor",
}


@dataclass(frozen=True)
class Surface:
    surface_type: str
    path: str
    scope: str
    depth: int
    reason: str

    def as_dict(self) -> dict[str, object]:
        return {
            "type": self.surface_type,
            "path": self.path,
            "scope": self.scope,
            "depth": self.depth,
            "reason": self.reason,
        }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Discover supported-language targets and likely command surfaces.",
    )
    parser.add_argument(
        "--root",
        default=".",
        help="Repository root to inspect (default: current directory).",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Print structured JSON instead of a human summary.",
    )
    return parser.parse_args()


def should_exclude_dir(name: str) -> bool:
    if name in EXCLUDED_DIR_NAMES:
        return True
    if name.startswith(".") and name not in {".shatter"}:
        return True
    return False


def load_package_json(path: Path) -> dict[str, object] | None:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return None


def docs_summary(repo_root: Path) -> tuple[list[dict[str, object]], list[str]]:
    doc_hits: dict[str, int] = {key: 0 for key in DOC_HINT_PATTERNS}
    scanned: list[str] = []

    for relative in DOC_FILES:
        path = repo_root / relative
        if not path.is_file():
            continue
        scanned.append(relative)
        try:
            content = path.read_text(encoding="utf-8")
        except OSError:
            continue
        for surface_type, pattern in DOC_HINT_PATTERNS.items():
            doc_hits[surface_type] += len(pattern.findall(content))

    hints = [
        {"type": surface_type, "mentions": mentions}
        for surface_type, mentions in sorted(
            doc_hits.items(),
            key=lambda item: (-item[1], item[0]),
        )
        if mentions > 0
    ]
    return hints, scanned


def discover_targets(repo_root: Path) -> tuple[list[dict[str, object]], list[str]]:
    targets: list[dict[str, object]] = []
    excluded_roots: list[str] = []

    for current_root, dir_names, file_names in os.walk(repo_root, topdown=True):
        current = Path(current_root)

        kept_dirs: list[str] = []
        for dir_name in sorted(dir_names):
            if should_exclude_dir(dir_name):
                excluded_roots.append(
                    os.path.relpath(current / dir_name, repo_root).replace(os.sep, "/")
                )
            else:
                kept_dirs.append(dir_name)
        dir_names[:] = kept_dirs

        markers = [
            marker for marker in sorted(SUPPORTED_MARKERS) if marker in file_names
        ]
        if not markers:
            continue

        rel_root = os.path.relpath(current, repo_root).replace(os.sep, "/")
        if rel_root == ".":
            rel_root = "."

        package_data = None
        if "package.json" in markers:
            package_data = load_package_json(current / "package.json")

        targets.append(
            {
                "root": rel_root,
                "markers": markers,
                "languages": sorted({SUPPORTED_MARKERS[marker] for marker in markers}),
                "package_name": (
                    package_data.get("name")
                    if isinstance(package_data, dict)
                    and isinstance(package_data.get("name"), str)
                    else None
                ),
            }
        )

    targets.sort(key=lambda target: (target["root"] != ".", target["root"]))
    return targets, sorted(set(excluded_roots))


def package_json_surface(path: Path, repo_root: Path, scope: str, depth: int) -> Surface | None:
    package_data = load_package_json(path)
    if not isinstance(package_data, dict):
        return None
    scripts = package_data.get("scripts")
    if not isinstance(scripts, dict) or not scripts:
        return None
    rel_path = os.path.relpath(path, repo_root).replace(os.sep, "/")
    return Surface(
        surface_type="package-json-scripts",
        path=rel_path,
        scope=scope,
        depth=depth,
        reason="package.json defines scripts",
    )


def detect_surfaces_for_target(
    target_root: Path,
    repo_root: Path,
    doc_hints: Iterable[dict[str, object]],
) -> list[dict[str, object]]:
    ordered_surfaces: list[Surface] = []
    hinted_types = {hint["type"] for hint in doc_hints}

    current = target_root
    depth = 0
    while True:
        scope = "local" if depth == 0 else "ancestor"

        package_surface = package_json_surface(
            current / "package.json",
            repo_root,
            scope,
            depth,
        )
        if package_surface:
            ordered_surfaces.append(package_surface)

        for file_name, surface_type in SURFACE_FILES.items():
            path = current / file_name
            if path.is_file():
                rel_path = os.path.relpath(path, repo_root).replace(os.sep, "/")
                reason = f"{file_name} present"
                if surface_type in hinted_types:
                    reason += "; reinforced by docs"
                ordered_surfaces.append(
                    Surface(
                        surface_type=surface_type,
                        path=rel_path,
                        scope=scope,
                        depth=depth,
                        reason=reason,
                    )
                )

        if current == repo_root:
            break
        parent = current.parent
        if parent == current or not str(parent).startswith(str(repo_root)):
            break
        current = parent
        depth += 1

    deduped: list[dict[str, object]] = []
    seen: set[tuple[str, str]] = set()
    for surface in sorted(
        ordered_surfaces,
        key=lambda item: (item.depth, item.scope != "local", item.surface_type, item.path),
    ):
        key = (surface.surface_type, surface.path)
        if key in seen:
            continue
        seen.add(key)
        deduped.append(surface.as_dict())
    return deduped


def build_report(repo_root: Path) -> dict[str, object]:
    doc_hints, docs_scanned = docs_summary(repo_root)
    targets, excluded_paths = discover_targets(repo_root)

    for target in targets:
        target_path = repo_root / target["root"]
        target["surfaces"] = detect_surfaces_for_target(target_path, repo_root, doc_hints)

    return {
        "repo_root": str(repo_root.resolve()),
        "docs_scanned": docs_scanned,
        "doc_hints": doc_hints,
        "target_count": len(targets),
        "requires_confirmation": len(targets) > 4,
        "targets": targets,
        "excluded_paths": excluded_paths,
    }


def print_human_summary(report: dict[str, object]) -> None:
    print(f"Repository: {report['repo_root']}")
    print(f"Targets: {report['target_count']}")
    print(f"Requires confirmation: {'yes' if report['requires_confirmation'] else 'no'}")

    doc_hints = report["doc_hints"]
    if doc_hints:
        hints = ", ".join(f"{hint['type']} ({hint['mentions']})" for hint in doc_hints)
        print(f"Doc hints: {hints}")
    else:
        print("Doc hints: none")

    for target in report["targets"]:
        print()
        print(
            f"- {target['root']} [{', '.join(target['languages'])}] "
            f"markers={', '.join(target['markers'])}"
        )
        surfaces = target["surfaces"]
        if not surfaces:
            print("  surfaces: none")
            continue
        for surface in surfaces:
            print(
                "  surface:"
                f" {surface['type']} at {surface['path']}"
                f" ({surface['scope']}, depth={surface['depth']})"
            )


def main() -> int:
    args = parse_args()
    repo_root = Path(args.root).resolve()
    report = build_report(repo_root)

    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print_human_summary(report)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
