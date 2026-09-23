# Task `sources:` and affected-gates omit real inputs (parity matrix, runtime crate, rust-fe tests/, shatter-llm): add a coverage meta test

- Priority: P2
- Type: task
- Labels: quality-gates,taskfile,agents,parity
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (partially covered by str-35vtk.36, str-qwua7.2)
- Source findings: protocol-parity-04, frontend-rust-07
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
Checksum caching and diff-scoped gate selection were added for speed with no
invariant that a relevant change must not be skipped. Several gates omit
inputs they read, so edits to those files are served from cache or never
select the gate.

## Current Code Facts
- `Taskfile.yml:245-263` `parity.sources` omit `protocol/parity-matrix.yaml`,
  `protocol/PARITY.md`, `scripts/validate-parity.py`, although
  `parity-governed` runs validate-parity over them.
- `shatter-rust/Taskfile.yml` `test` sources: `src/**/*.rs`, `Cargo.toml`,
  `Cargo.lock`, `nextest-standalone.toml` — omit `tests/**/*.rs`
  (`tests/codegen_parity.rs`) and `../shatter-rust-runtime/**` (executor tests
  build harnesses against it via `find_runtime_crate_path`, executor.rs:1198).
- `python3 -c` on `scripts/affected-gates.py` `select_gates`:
  `['shatter-rust-runtime/src/lib.rs']` -> `[smoke, rust-rt:clippy,
  rust-rt:test, e2e-rust]` (no rust-fe:test);
  `['shatter-llm/src/jev.rs']` -> `[smoke, check]`, and `check` runs no
  shatter-llm clippy/tests (`.github/workflows/ci.yml:90-103` adds them
  separately).
- Related broader gap (other L4 findings): cli:test sources omit
  `shatter-cli/tests/**`, `templates/`, `build.rs`; core:test-ignored runs E2E
  suites but lists no frontend sources.

## Acceptance Criteria
- Missing globs added to `parity`, `rust-fe:test` sources.
- `affected-gates.py` maps `shatter-rust-runtime/` to `rust-fe:test` too and has
  an explicit `shatter-llm/` rule selecting llm clippy+test tasks (create them
  if str-35vtk.36 has not).
- A meta test parses the Taskfiles (YAML, not `task --list-all --json`) and
  asserts, for a declared table of task → required input globs, that each
  glob appears in `sources:`; plus table-driven `select_gates` cases for the
  paths above. Both fail on today's tree.

## Out of Scope
Removing Task checksum caching entirely (design decision under str-qwua7.2).
