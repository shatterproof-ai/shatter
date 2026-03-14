# Plan: Placeholder Contract Audit (flt-it8.32.11)

## Context

Docs-truthfulness policy requires that documentation doesn't claim features are shipped while code still has placeholder handlers. No automated enforcement exists yet. The governance hardening plan (Phase 4) specifies `scripts/ci/check-placeholders.py` as the deliverable.

Current state: only ONE placeholder exists in the codebase — the CLI voice command (`api/cmd/flotsam/main.go:292`) prints "not yet implemented". No doc/code mismatches exist today (README mentions voice via Android app, not CLI).

## Deliverables

1. `scripts/ci/check-placeholders.py` — scans Go source for placeholders, cross-references with doc claims
2. `scripts/ci/test_check_placeholders.py` — unit tests
3. Any doc fixes if real mismatches are found

## Script Design

### Three phases

1. **Scan Go source** (`api/**/*.go`) for placeholder patterns:
   - HTTP 501 / `StatusNotImplemented`
   - "not implemented", "not yet implemented", "coming soon" strings
   - TODO/FIXME in handler dirs (`graph/resolver/`, `internal/router/`, `cmd/`)
   - Exclude: `graph/generated/`, `graph/model/`, `*_test.go`

2. **Scan docs** for feature-availability claims:
   - Files: `README.md`, `CLAUDE.md`, `api/CLAUDE.md`, `web/CLAUDE.md`, `docs/specs/**/*.md`, `docs/policies/**/*.md`
   - Claim words: "implemented", "shipped", "available", "complete", "ready", etc.
   - Exclude lines qualified by: "not yet", "planned", "future", "will be", etc.
   - Exclude `docs/plans/` entirely (plans describe future work)

3. **Cross-reference** using feature keywords (voice, bookmark, mcp, search, etc.)
   - Mismatch = placeholder's feature hint overlaps a doc claim's feature hints

### CLI interface (matches `detect-risk.py` conventions)
- `--json` for CI output
- `--strict` exits 1 on ANY placeholder (even without doc mismatch)
- Exit 0 = clean, Exit 1 = mismatches found
- `repo_root = Path(__file__).resolve().parents[2]`

### Data structures
- `Placeholder(file, line, category, text, feature_hint)`
- `DocClaim(file, line, text, claim_word, feature_hints)`
- `Mismatch(placeholder, claim, feature)`

## Tests (`test_check_placeholders.py`)

Follow `test_detect_risk.py` pattern: `importlib.util` import, `unittest`, temp dirs for filesystem tests, `unittest.mock.patch` for main().

Key test classes:
- `TestScanCodePlaceholders` — detects patterns, respects exclusions, extracts hints
- `TestScanDocClaims` — detects claims, excludes qualified lines, extracts hints
- `TestFindMismatches` — keyword overlap logic
- `TestMainExitCode` — exit codes, JSON output, strict mode

## Critical files
- `scripts/ci/detect-risk.py` — pattern to follow
- `scripts/ci/test_detect_risk.py` — test pattern to follow
- `api/cmd/flotsam/main.go:292` — known placeholder (voice command)
- `README.md` — primary doc to scan

## Verification
```bash
python3 scripts/ci/check-placeholders.py          # should exit 0 (no mismatches)
python3 scripts/ci/check-placeholders.py --json    # JSON output
python3 scripts/ci/check-placeholders.py --strict  # exit 1 (voice placeholder exists)
python3 -m pytest scripts/ci/test_check_placeholders.py -v
```
