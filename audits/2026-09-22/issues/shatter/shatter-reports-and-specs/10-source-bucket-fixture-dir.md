---
slug: source-bucket-fixture-dir
kind: new
title: "Source-bucket classifier labels production packages named 'fixture' as fixture_sample, zeroing the production denominator"
priority: P3
type: bug
labels: [scan, report, classification, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Source-bucket classifier labels production packages named `fixture` as fixture_sample, zeroing the production denominator

## Problem

Scanning zolem's `internal/fixture` package, which is production code (a fixture loader and selector), reports `source_set.fixture_sample {file_count: 10, line_count: 1279}`, `production_ish {0, 0}` and `productionish_source_lines: 0`. Every function gets `source_bucket: fixture_sample`. The production coverage denominator becomes zero, and coverage summaries for the package are meaningless.

This is the same class of bug as closed str-9awj, which fixed the same segment-name heuristic for `specs` directories. The heuristic is still applied to other names.

## Evidence

Re-checked on 2026-09-23 in the audit worktree (HEAD 56c86168):

- `shatter-core/src/source_bucket.rs:253-267`: `FIXTURE_DIRS = ["testdata", "test-data", "fixtures", "fixture", "examples", "example", "samples", "sample", "__fixtures__"]`, and `is_fixture_sample` returns true if **any** path segment matches. So `internal/fixture/loader.go`, `pkg/sample/...` and `internal/example/...` are all classified as fixtures, wherever they sit.
- Audit evidence (on branch `audit-2026-09-22` until the audit reports land): `audits/2026-09-22/goals-runs/zolem-fixture-default.json` has `codebase.source_set.fixture_sample = {file_count: 10, line_count: 1279}`, `production_ish = {0, 0}` and `codebase.productionish_source_lines = 0` (re-read 2026-09-23); write-up in `audits/2026-09-22/areas/goals.md` item 16 (goals-16).

## Acceptance criteria

- [ ] Classification follows conventions instead of any-segment names: `testdata/`, `__fixtures__/`, `fixtures/` inside a test tree, `*_test.go`, top-level `examples/`. A package directory named `fixture`, `sample` or `example` below a production source root, in Go, TS or Rust, is not a fixture by itself.
- [ ] `shatter.config.json` / `.shatter/config.yaml` can override the bucket per path glob, and the override is documented (README or SPEC config section).
- [ ] A unit test classifies a production `internal/fixture/loader.go` as `production_ish`, and a property or table test covers the remaining names. The test fails on current code; record both runs.
- [ ] The str-9awj regression (`internal/specs`) stays covered.

## Suggested approach

Anchor the fixture rule to test roots and the repo root (or the scan root), not to any segment. Add a config override map from glob to bucket, resolved before the heuristic.

## Out of scope

Other source-bucket categories (generated, declaration_only) unless they share the same any-segment rule.

## Related

str-9awj (closed; same class for `specs`, gets a reopen-note pointing here), str-jeen.37, str-jeen.38. Source finding: goals-16 (confirmed).
