#!/usr/bin/env python3
"""Select continuous prereleases that are outside the retention window."""

from __future__ import annotations

import argparse
import json
import sys
from datetime import datetime


def parse_created_at(value: str) -> datetime:
    return datetime.fromisoformat(value.replace("Z", "+00:00"))


def deletion_candidates(
    releases: list[dict[str, object]],
    keep_recent: int,
    keep_monthly: int,
    protected_tags: set[str],
) -> list[str]:
    continuous = [
        release
        for release in releases
        if release.get("isPrerelease")
        and isinstance(release.get("tagName"), str)
        and release["tagName"].startswith("continuous-")
    ]
    continuous.sort(key=lambda release: parse_created_at(str(release["createdAt"])), reverse=True)

    keep = {str(release["tagName"]) for release in continuous[:keep_recent]}
    keep.update(protected_tags)

    monthly_seen: set[str] = set()
    for release in continuous[keep_recent:]:
        created = parse_created_at(str(release["createdAt"]))
        bucket = created.strftime("%Y-%m")
        if bucket in monthly_seen or len(monthly_seen) >= keep_monthly:
            continue
        monthly_seen.add(bucket)
        keep.add(str(release["tagName"]))

    return [str(release["tagName"]) for release in continuous if str(release["tagName"]) not in keep]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--keep-recent", type=int, default=30)
    parser.add_argument("--keep-monthly", type=int, default=12)
    parser.add_argument("--protect", default="")
    args = parser.parse_args()

    protected = {tag.strip() for tag in args.protect.split(",") if tag.strip()}
    releases = json.load(sys.stdin)
    for tag in deletion_candidates(releases, args.keep_recent, args.keep_monthly, protected):
        print(tag)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
