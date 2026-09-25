#!/usr/bin/env python3
"""Verify that every audit evidence path cited in an audit's issue drafts exists at a git ref.

Usage: scripts/check-audit-evidence-paths.py <audit-date> [--ref origin/main]

Scans audits/<date>.md and audits/<date>/issues/**/*.md for paths of the form
`audits/<date>/...`, strips `:line` fragments and trailing punctuation, and runs
`git cat-file -e <ref>:<path>` on each. Paths under untracked-by-design dirs
(goals-runs/, sessions/sessions.json) are reported separately and do not fail.
Exits 1 if any other cited path is missing at the ref.
"""
import argparse
import pathlib
import re
import subprocess
import sys

UNTRACKED_BY_DESIGN = ("goals-runs", "sessions/sessions.json")


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("date")
    ap.add_argument("--ref", default="origin/main")
    args = ap.parse_args()

    root = pathlib.Path(subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip())
    prefix = f"audits/{args.date}"
    sources = [root / f"{prefix}.md", *sorted((root / prefix / "issues").rglob("*.md"))]
    pattern = re.compile(re.escape(prefix) + r"(?:\.md|/[A-Za-z0-9_./@+-]*)")

    cited: dict[str, str] = {}
    for src in sources:
        if not src.is_file():
            continue
        for m in pattern.finditer(src.read_text(encoding="utf-8")):
            path = re.sub(r":\d+(-\d+)?$", "", m.group(0)).rstrip(".,);:`'\"")
            if path.endswith("/"):
                path = path.rstrip("/")
            cited.setdefault(path, str(src.relative_to(root)))

    missing, skipped = [], []
    for path, src in sorted(cited.items()):
        rel = path[len(prefix) + 1:] if path.startswith(prefix + "/") else ""
        if any(rel.startswith(u) for u in UNTRACKED_BY_DESIGN):
            skipped.append(path)
            continue
        ok = subprocess.run(["git", "cat-file", "-e", f"{args.ref}:{path}"], cwd=root,
                            capture_output=True).returncode == 0
        if not ok:
            # A citation may name a file stem (`scan-mix` for scan-mix.json/.html).
            listing = subprocess.run(["git", "ls-tree", "--name-only", args.ref, path + ".*"],
                                     cwd=root, capture_output=True, text=True).stdout
            parent = path.rsplit("/", 1)[0]
            names = subprocess.run(["git", "ls-tree", "--name-only", f"{args.ref}:{parent}"],
                                   cwd=root, capture_output=True, text=True).stdout.split()
            stem = path.rsplit("/", 1)[1]
            ok = bool(listing.strip()) or any(n.startswith(stem + ".") for n in names)
        if not ok:
            missing.append((path, src))

    for path, src in missing:
        print(f"MISSING {path}  (cited in {src})")
    print(f"checked {len(cited) - len(skipped)} cited paths at {args.ref}; "
          f"missing: {len(missing)}; untracked-by-design (not checked): {len(skipped)}")
    return 1 if missing else 0


if __name__ == "__main__":
    sys.exit(main())
