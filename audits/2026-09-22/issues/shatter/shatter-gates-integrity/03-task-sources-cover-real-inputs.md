---
slug: task-sources-cover-real-inputs
kind: new
title: "Task `sources:` omit real test inputs (CLI tests/templates/build.rs, embedded frontends, E2E frontend trees, parity matrix, runtime crate, rust-fe tests/): add the globs and a sources-coverage meta test"
priority: P1
type: bug
labels: [quality-gates, taskfile, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Task `sources:` omit real test inputs: add the globs and a sources-coverage meta test

## Problem

go-task's checksum cache skips a leaf when none of its declared `sources:` has changed. Several test leaves omit files that affect their result, so an edit that matters can be answered with a cached pass. The checksum caching was added for speed (str-35vtk) without the invariant "a relevant change must never be skipped", and without any test for it. This issue merges audit drafts code/03 and agent/20 (report §15.1). The affected-selector half of agent/20 is filed separately as `affected-gates-routing`.

Related: str-qwua7.2 (open) questions checksum caching in general and covers executed-vs-cached receipts. It does not list the missing globs or add a coverage test.

## Evidence (re-verified 2026-09-23 against the audit snapshot; only shatter-core/Taskfile.yml has changed on main since)

- **cli:test**, at `shatter-cli/Taskfile.yml:16-24`, and **cli:test-fast** (`:35`ff) list only `src/**/*.rs`, `Cargo.toml`, `../Cargo.lock`, core `src/**/*.rs`, core `Cargo.toml` and `../.config/nextest.toml`. They miss:
  - `shatter-cli/tests/**` (30 entries)
  - `shatter-cli/templates/**`: askama templates `explore_fn.md` and `scan.md`, used at `render.rs:17,40`
  - `shatter-cli/build.rs`, which embeds shatter-ts (`build.rs:90`) and shatter-go (`build.rs:163`)
  - the embedded frontend trees `shatter-ts/src/**`, `shatter-go/**/*.go` and `shatter-core/build.rs`
- **core:test-ignored**, at `shatter-core/Taskfile.yml:37-45`, runs the Go, TS and Rust E2E suites (`--run-ignored all` / `--include-ignored`), but lists no frontend source trees (`shatter-ts/src`, `shatter-go`, `shatter-rust/src`, `shatter-rust-runtime/src`). `test-ignored-fast` has the same gap.
- **workspace-test** (test-standard), at `Taskfile.yml:115-128`, runs the Go and Rust E2E suites. It lists `shatter-cli/**/*.rs` and `shatter-llm/**/*.rs`, but no frontend sources and no `shatter-cli/templates/**`.
- **parity**, at `Taskfile.yml:245-263`: its sources (`:252-261`) omit `protocol/parity-matrix.yaml`, `protocol/PARITY.md`, `scripts/validate-parity.py` and `protocol/conformance/run_golden_tests.py`, although `parity-governed` (`:265-276`) runs `validate-parity.py` and `run_golden_tests.py` over them. `affected-gates.py` selects `parity` for any `protocol/` change, and Task then skips it as up to date when only those files changed.
- **rust-fe:test**, at `shatter-rust/Taskfile.yml:13-19`, lists `src/**/*.rs`, `Cargo.toml`, `Cargo.lock` and `../.config/nextest-standalone.toml`. It omits `tests/**/*.rs` (`tests/codegen_parity.rs`) and `../shatter-rust-runtime/src/**`. The executor tests build harnesses against the runtime crate through `find_runtime_crate_path` (`shatter-rust/src/executor.rs`, around line 1198).
- **ts:test**, at `shatter-ts/Taskfile.yml:38-49`, omits `jest.config.js`, which exists.
- **shatter-llm**: `check` has no shatter-llm test leaf (str-35vtk.36, open, P3). CI covers it with two extra steps (`.github/workflows/ci.yml:99-103`). Any llm test task added under str-35vtk.36 must declare `shatter-llm/**` sources. The meta test below must cover it once that task exists.

## Acceptance criteria

- [ ] Each task named above declares the missing globs, including the `-fast` twins, which must stay in sync per the existing wiring tests.
- [ ] A new meta test, wired into `meta` (for example extending `scripts/test_test_tier_wiring.py` or a new `scripts/test_task_sources_coverage.py`), parses the Taskfiles as YAML. It does **not** use `task --list-all --json`; see str-qwua7.3. It asserts:
  1. For a declared table of test task → required input globs (crate `tests/`, `templates/`, `build.rs`, each executed or embedded frontend tree, the parity matrix, validator scripts and the runtime crate), each glob appears in that task's `sources:`.
  2. Every git-tracked file under a test task's crate or declared input roots (`git ls-files`) matches some `sources:` glob of a task that tests it. Files that do not feed a test go in an explicit allowlist with a reason.
- [ ] Proof: the meta test fails on the current tree (paste the failing assertion into the close reason) and passes after the globs are added.
- [ ] Forced-execution proof: after the fix, `touch shatter-cli/templates/scan.md && task cli:test` executes tests, and `touch protocol/parity-matrix.yaml && task parity` executes `validate-parity.py`. Neither prints `is up to date`. Record both in the close reason.

## Suggested approach

Add the globs first. Then write the table-driven meta test, with the `git ls-files` sweep as its second half. Alternatively, drop Task checksum caching for test leaves entirely and rely on cargo, go and jest incrementality. If you choose that, record the decision on str-qwua7.2, and the meta test shrinks to asserting that test leaves have no `sources:`.

## Out of scope

- Diff-scoped gate selection in `scripts/affected-gates.py` (`affected-gates-routing`).
- The root cause of the meta-stage checksum poisoning (str-qwua7.3).
- Folding shatter-llm into `task check` (str-35vtk.36). This issue only requires its sources once the task exists.

## Metadata

- Priority: P1. Type: bug. Size: S-M.
- Labels: quality-gates, taskfile, parity, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: none. It is independent of str-qwua7.3, but the forced-execution proof is only meaningful once str-qwua7.3 is fixed.
- Related: str-qwua7.2, str-35vtk, str-35vtk.36.
- Source findings: tests-ci-04, frontend-rust-07 (sources part), protocol-parity-04. Drafts shatter-code/03 and shatter-agent/20.
