# Task `sources:` omit real test inputs (CLI tests/templates/build.rs, embedded frontends, frontend sources for E2E), so gates skip after relevant edits

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P1 |
| labels | quality-gates,taskfile,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-qwua7.2, str-35vtk |
| source findings | tests-ci-04, frontend-rust-07 (sources part) |

<!-- body -->
## Problem

Go-task checksum caching skips a test leaf when none of its declared `sources:` changed. Several leaves omit files that affect their result, so a relevant edit can be served a cached pass.

## Current code facts / evidence

- `shatter-cli/Taskfile.yml` test sources: `src/**/*.rs`, Cargo.toml/lock, core src, nextest.toml. Missing: `shatter-cli/tests/**` (30 entries), `shatter-cli/templates/**` (askama, `render.rs:17,40`), `shatter-cli/build.rs` (embeds shatter-ts at :90 and shatter-go at :163).
- `shatter-core/Taskfile.yml:37-45` `test-ignored` runs Go/TS/Rust E2E suites via `--run-ignored all` but lists no frontend source trees.
- `Taskfile.yml:115-128` `workspace-test` (test-standard) runs Go/Rust E2E but lists no frontend sources or templates.
- `shatter-rust/Taskfile.yml:13-19` test sources omit `tests/**/*.rs` (e.g. `tests/codegen_parity.rs`) and `../shatter-rust-runtime/src/**` (executor tests build harnesses against it).
- `shatter-ts/Taskfile.yml:38-49` omits `jest.config.js`.

## Acceptance criteria

- Each listed task's `sources:` include the missing globs above.
- New meta test: for each test leaf, assert its sources cover its crate's `tests/`, `templates/`, `build.rs`, and every frontend tree it executes (table-driven).
- Manual check: touching `shatter-cli/templates/<any>.md` then running `task cli:test` executes tests rather than reporting up to date.

## Suggested approach

Add the globs. Consider dropping Task checksum caching for test leaves entirely and relying on cargo/go/jest incrementality; if you keep caching, the meta test enforces coverage.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: Affected-gates routing (draft 04).
- Size: S

## References

- Audit findings: tests-ci-04, frontend-rust-07 (sources part) (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-qwua7.2, str-35vtk
