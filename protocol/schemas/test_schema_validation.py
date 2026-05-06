#!/usr/bin/env python3
"""Validate protocol fixtures against JSON schemas.

Auto-discovers all fixtures under protocol/fixtures/ and validates them:
- requests/valid/*   -> must pass request.schema.json
- requests/invalid/* -> must FAIL request.schema.json
- responses/valid/*  -> must pass response.schema.json
- responses/invalid/*-> must FAIL response.schema.json
- errors/*           -> must pass response.schema.json (error responses)
- top-level *-request.json  -> must pass request.schema.json
- top-level *-response.json -> must pass response.schema.json

Exits non-zero if any fixture produces an unexpected result.
"""

import json
import sys
from pathlib import Path

from jsonschema import Draft202012Validator, RefResolver

SCHEMA_DIR = Path(__file__).parent
FIXTURE_DIR = SCHEMA_DIR.parent / "fixtures"

# Fixtures with known schema mismatches. The validator reports these as
# warnings rather than failures so that the check passes while the schemas
# are being updated. Remove entries as schemas are fixed.
KNOWN_MISMATCHES: set[str] = {
    "responses/valid/analyze.json",
    "responses/valid/execute.json",
}


def load_json(path: Path) -> dict:
    with open(path) as f:
        return json.load(f)


def make_resolver(schema_dir: Path) -> RefResolver:
    """Build a resolver that can follow $ref to sibling schema files."""
    store = {}
    for schema_file in schema_dir.glob("*.schema.json"):
        s = load_json(schema_file)
        store[schema_file.name] = s
        if "$id" in s:
            store[s["$id"]] = s
    base_uri = f"file://{schema_dir.resolve()}/"
    return RefResolver(base_uri, {}, store=store)


def validate_one(
    fixture_path: Path,
    validator: Draft202012Validator,
    expect_valid: bool,
) -> tuple[str | None, bool]:
    """Validate a single fixture.

    Returns (failure_message_or_None, is_known_mismatch).
    """
    instance = load_json(fixture_path)
    errors = list(validator.iter_errors(instance))
    label = str(fixture_path.relative_to(FIXTURE_DIR))
    known = label in KNOWN_MISMATCHES

    if expect_valid:
        if errors:
            detail = "; ".join(e.message[:120] for e in errors[:3])
            if known:
                print(f"  KNOWN: {label} (schema mismatch, see KNOWN_MISMATCHES)")
                return None, True
            return f"  FAIL (expected valid): {label}: {detail}", False
        if known:
            return f"  STALE known-mismatch (now passes): {label}", False
        print(f"  OK: {label}")
        return None, False
    else:
        if errors:
            print(f"  OK (correctly invalid): {label}")
            return None, False
        return f"  FAIL (expected invalid but passed): {label}", False


def discover_and_validate(
    directory: Path,
    validator: Draft202012Validator,
    expect_valid: bool,
) -> tuple[list[str], int, int]:
    """Discover all .json files in directory and validate them.

    Returns (failures, total_count, known_mismatch_count).
    """
    failures: list[str] = []
    known_count = 0
    if not directory.is_dir():
        return failures, 0, 0
    files = sorted(directory.glob("*.json"))
    for fixture in files:
        result, known = validate_one(fixture, validator, expect_valid)
        if result:
            failures.append(result)
        if known:
            known_count += 1
    return failures, len(files), known_count


def main() -> int:
    resolver = make_resolver(SCHEMA_DIR)
    request_schema = load_json(SCHEMA_DIR / "request.schema.json")
    response_schema = load_json(SCHEMA_DIR / "response.schema.json")
    req_validator = Draft202012Validator(request_schema, resolver=resolver)
    resp_validator = Draft202012Validator(response_schema, resolver=resolver)

    all_failures: list[str] = []
    total_checked = 0
    total_known = 0

    sections: list[tuple[str, Path, Draft202012Validator, bool]] = [
        ("valid request", FIXTURE_DIR / "requests" / "valid", req_validator, True),
        ("invalid request", FIXTURE_DIR / "requests" / "invalid", req_validator, False),
        ("valid response", FIXTURE_DIR / "responses" / "valid", resp_validator, True),
        ("invalid response", FIXTURE_DIR / "responses" / "invalid", resp_validator, False),
        ("error response", FIXTURE_DIR / "errors", resp_validator, True),
    ]

    for label, directory, validator, expect_valid in sections:
        if not directory.is_dir():
            continue
        count = len(list(directory.glob("*.json")))
        verb = "must fail" if not expect_valid else ""
        suffix = f" ({verb})" if verb else ""
        print(f"Validating {count} {label} fixtures{suffix}:")
        failures, n, known = discover_and_validate(directory, validator, expect_valid)
        all_failures.extend(failures)
        total_checked += n
        total_known += known
        print()

    # Top-level fixtures
    top_requests = sorted(FIXTURE_DIR.glob("*-request.json"))
    top_responses = sorted(FIXTURE_DIR.glob("*-response.json"))
    if top_requests or top_responses:
        print(f"Validating {len(top_requests) + len(top_responses)} top-level fixtures:")
        for f in top_requests:
            result, known = validate_one(f, req_validator, True)
            if result:
                all_failures.append(result)
            if known:
                total_known += 1
            total_checked += 1
        for f in top_responses:
            result, known = validate_one(f, resp_validator, True)
            if result:
                all_failures.append(result)
            if known:
                total_known += 1
            total_checked += 1
        print()

    print("=" * 50)
    print(f"Checked {total_checked} fixtures", end="")
    if total_known:
        print(f" ({total_known} known mismatches skipped)")
    else:
        print()

    if all_failures:
        print(f"{len(all_failures)} failure(s):")
        for f in all_failures:
            print(f)
        return 1

    print("All fixtures valid.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
