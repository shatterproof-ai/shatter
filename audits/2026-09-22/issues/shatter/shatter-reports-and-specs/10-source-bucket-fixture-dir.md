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

Re-checked on 2026-09-23 in the audit worktree (HEAD 793f2b0b; code identical to 56c86168):

- `shatter-core/src/source_bucket.rs:253-267`: `FIXTURE_DIRS = ["testdata", "test-data", "fixtures", "fixture", "examples", "example", "samples", "sample", "__fixtures__"]`, and `is_fixture_sample` returns true if **any** path segment matches. So `internal/fixture/loader.go`, `pkg/sample/...` and `internal/example/...` are all classified as fixtures, wherever they sit.
- `shatter-core/src/source_bucket.rs:145-170` (`classify_path`): classification is path-only and works on whatever path string the caller passes, lower-cased and split on `/`. Callers include `report.rs:908`, `:1440`, `:6188` and `status_export.rs:955`, `:1115`; the implementer must check which of them pass absolute paths (the zolem report used absolute `/tmp/...` paths elsewhere) and relativize before classifying. Precedence is policy_excluded > generated > unsupported > declaration_only > fixture_sample > test_spec > production_ish.
- Audit evidence (on branch `audit-2026-09-22` until the audit reports land): `audits/2026-09-22/goals-runs/zolem-fixture-default.json` has `codebase.source_set.fixture_sample = {file_count: 10, line_count: 1279}`, `production_ish = {0, 0}` and `codebase.productionish_source_lines = 0` (re-read 2026-09-23); write-up in `audits/2026-09-22/areas/goals.md` item 16 (goals-16).

## Classification contract

- The classifier works on the path relative to the project root (the root Shatter already detects; `--project-dir` overrides it). A scan started in a subdirectory classifies each file the same way as a scan started at the root. Files outside the project root keep today's behavior.
- A file is `fixture_sample` when any of these holds: a segment is `testdata`, `test-data` or `__fixtures__`; a segment is `fixtures`, `fixture`, `samples` or `sample` **and** an earlier segment is a test directory (`test`, `tests`, `__tests__`); the first segment (relative to the project root) is `examples`, `example`, `samples` or `sample`.
- Otherwise a directory named `fixture`, `fixtures`, `sample`, `samples`, `example` or `examples` below a production source root is not a fixture by itself, in Go, TS or Rust.
- Precedence between buckets does not change.

## Acceptance criteria

- [ ] The contract above is implemented in `source_bucket.rs`, and the module docs describe it.
- [ ] A table test classifies at least: `internal/fixture/loader.go` → production_ish; `pkg/sample/x.go` → production_ish; `internal/example/x.ts` → production_ish; `testdata/a.go`, `x/testdata/a.go`, `src/__fixtures__/a.ts`, `tests/fixtures/a.rs`, `examples/a.ts` → fixture_sample; `internal/specs/x.go` → production_ish (the str-9awj regression). The rows that change fail on current code; record both runs in the close comment.
- [ ] A test runs classification for the same file from the project root and from a subdirectory (scan root below the project root) and gets the same bucket.
- [ ] A proptest checks that inserting a `fixture`/`sample`/`example` segment into a production path below its first segment never changes the bucket, unless a test directory precedes it.
- [ ] Re-running the zolem `internal/fixture` scan gives a non-zero `production_ish` count; the close comment records the before/after `source_set`.
- [ ] Side effect check: the examples corpus lives at `/home/ketan/project/examples`, so today an absolute path through it contains an `examples` segment and may be bucketed `fixture_sample`. The close comment records the `source_set` of a scan of `standalone/ts/` before and after, and `task gauntlet` and `task walkthrough` pass (output recorded); any change in their reports is explained.
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Out of scope

- A user-configurable bucket override per path glob (source-bucket-config-override).
- Other source-bucket categories (generated, declaration_only) unless they share the same any-segment rule.

## Related

str-9awj (closed; same class for `specs`, gets a reopen-note pointing here), str-jeen.37, str-jeen.38, source-bucket-config-override. Source finding: goals-16 (confirmed).
