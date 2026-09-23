# Source-bucket classifier labels production packages named `fixture` as fixture_sample, zeroing the production denominator

- Priority: P3
- Type: bug
- Labels: scan,report,classification
- Tracker action: new issue (same class as closed str-9awj for `specs` directories)
- Related: str-9awj, str-jeen.37, str-jeen.38
- Source findings: audit 2026-09-22 goals-16 (confirmed)

<!-- body -->
## Problem
Scanning zolem's `internal/fixture` package (production code: a fixture loader and selector) reports `source_set.fixture_sample {file_count:10, line_count:1279}`, `production_ish {0,0}` and `productionish_source_lines: 0`. Every function has `source_bucket: fixture_sample`.

## Current code facts
- `shatter-core/src/source_bucket.rs:253-266` treats any path segment named `fixture` (singular) as a fixture directory, the same segment-name heuristic that str-9awj fixed for `specs`.

## Acceptance criteria
- Classification relies on conventions: `testdata/`, `__fixtures__/`, `fixtures/` inside test trees, `*_test.go`, `examples/`. A top-level package directory named `fixture` in Go, TS or Rust is not classified as a fixture by itself.
- `shatter.config.json` / `.shatter/config.yaml` can override the bucket per path glob. Document it.
- A unit test covers a production `internal/fixture/loader.go` classified as production-ish.
