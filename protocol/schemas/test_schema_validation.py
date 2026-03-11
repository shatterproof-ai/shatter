#!/usr/bin/env python3
"""Validate protocol fixtures against JSON schemas.

Exits non-zero if any fixture fails validation, printing which ones failed.
"""

import json
import sys
from pathlib import Path

from jsonschema import Draft202012Validator, RefResolver

SCHEMA_DIR = Path(__file__).parent
FIXTURE_DIR = SCHEMA_DIR.parent / "fixtures"

# Each entry: (fixture filename, schema filename)
REQUEST_FIXTURES = [
    "setup-request.json",
    "teardown-request.json",
]

RESPONSE_FIXTURES = [
    "setup-response.json",
    "teardown-ack-response.json",
]


def load_json(path: Path) -> dict:
    with open(path) as f:
        return json.load(f)


def make_resolver(schema_dir: Path) -> RefResolver:
    """Build a resolver that can follow $ref to sibling schema files."""
    store = {}
    for schema_file in schema_dir.glob("*.schema.json"):
        s = load_json(schema_file)
        # Key by filename (used in $ref like "mock-config.schema.json")
        store[schema_file.name] = s
        # Also key by $id if present
        if "$id" in s:
            store[s["$id"]] = s
    base_uri = f"file://{schema_dir.resolve()}/"
    return RefResolver(base_uri, {}, store=store)


def validate_fixtures(
    fixture_names: list[str],
    schema_name: str,
    resolver: RefResolver,
) -> list[str]:
    """Validate fixtures against a schema. Returns list of failure messages."""
    schema = load_json(SCHEMA_DIR / schema_name)
    validator = Draft202012Validator(schema, resolver=resolver)
    failures = []
    for name in fixture_names:
        fixture_path = FIXTURE_DIR / name
        if not fixture_path.exists():
            failures.append(f"  MISSING: {name}")
            continue
        instance = load_json(fixture_path)
        errors = list(validator.iter_errors(instance))
        if errors:
            detail = "; ".join(e.message for e in errors[:3])
            failures.append(f"  FAIL: {name}: {detail}")
        else:
            print(f"  OK: {name}")
    return failures


def main() -> int:
    resolver = make_resolver(SCHEMA_DIR)
    all_failures: list[str] = []

    print("Validating request fixtures against request.schema.json:")
    all_failures.extend(
        validate_fixtures(REQUEST_FIXTURES, "request.schema.json", resolver)
    )

    print("Validating response fixtures against response.schema.json:")
    all_failures.extend(
        validate_fixtures(RESPONSE_FIXTURES, "response.schema.json", resolver)
    )

    if all_failures:
        print(f"\n{len(all_failures)} failure(s):")
        for f in all_failures:
            print(f)
        return 1

    print("\nAll fixtures valid.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
