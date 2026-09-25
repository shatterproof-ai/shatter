---
slug: rust-runtime-path-and-doctor
kind: new
title: "Rust explore outside the source tree fails per function on the undocumented SHATTER_RUNTIME_PATH, with the language labelled `any`"
priority: P2
type: bug
labels: [rust-frontend, docs, ux, install, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Rust explore outside the source tree fails per function on the undocumented SHATTER_RUNTIME_PATH, with the language labelled `any`

## Problem

A user who installs `shatter-rust` on PATH (as the missing-frontend hint tells them to) and explores a `.rs` target outside the shatter source tree gets:

1. `execute error (FileNotFound): cannot locate shatter-rust-runtime crate; set SHATTER_RUNTIME_PATH`, **once per function**.
2. A "Failure impact" table whose only row is labelled `any`, with no `rust` row, although every target is `.rs`.
3. No documentation of `SHATTER_RUNTIME_PATH`: it is not in README, QUICKSTART, SPEC or any `--help` text, and the error does not say what value to set or where the CLI looked.

Scope note: this draft was narrowed during the cross-check. The doctor and missing-frontend-hint parts of the original finding are owned elsewhere:

- Rust-frontend resolution and runtime-crate location in `shatter doctor`: str-qwua7.40 (see doctor-rust-runtime-note in this bucket).
- The missing-frontend hint printed twice and its length: str-qwua7.13 (see rust-hint-once-note in this bucket).
- Toolchain and sandbox/host-write readiness in `shatter doctor`: doctor-execution-readiness (this bucket).

## Evidence

Re-verified against the audit worktree (code at HEAD 56c86168):

- `shatter-rust/src/executor.rs:1198-1223` `find_runtime_crate_path()`: reads `SHATTER_RUNTIME_PATH` first (used only if `<path>/Cargo.toml` exists), then walks up to five ancestors of the **`shatter-rust` executable** (`std::env::current_exe()`), not the cwd, looking for a sibling `shatter-rust-runtime/Cargo.toml`. Otherwise it returns `"cannot locate shatter-rust-runtime crate; set SHATTER_RUNTIME_PATH"`. So a `shatter-rust` built inside a checkout finds the crate from any cwd, and a copied or installed binary never does.
- `shatter-cli/src/commands/build_frontend.rs:607-630` is the only other place that mentions the variable (warnings during `build-frontend`).
- `grep -n SHATTER_RUNTIME_PATH README.md QUICKSTART.md SPEC.md` returns no matches.
- `shatter-cli/src/commands/explore.rs:3240-3278`: the failure-impact rollup has per-language classifier rows only for Go (`BuildFailed`) and TS (`BuildFailed`/`RuntimeFailed`). Rust failures only reach the outcome-only `("any", tok)` row.
- Transcript `audits/2026-09-22/cli-ux-transcripts/rust-explore2.out` / `.err`: the runtime-crate error three times (three functions), and the failure-impact row `| runtime_failed | any | 3 | 1 | 47 | 47 | 100.0% |`.
- Why tests miss it: the E2E Rust suite does not rely on discovery at all. `shatter-core/tests/e2e_concolic_rust.rs:122-125` and `shatter-core/tests/support/rust_frontend_harness.rs:77` set `SHATTER_RUNTIME_PATH` explicitly, and the test binaries live under the workspace `target/`, where the executable-ancestor walk would also succeed. No test runs a relocated `shatter-rust` with the variable unset.
- Source findings: audit 2026-09-22 cli-ux-10 (confirmed, P2). Area evidence: `audits/2026-09-22/areas/cli-ux.md` F10.

## Acceptance criteria

- [ ] When the runtime crate cannot be located, the error is reported **once per run** (not once per function) and the remaining Rust functions in that run are reported as skipped for the same reason. The message names `SHATTER_RUNTIME_PATH`, gives an example value (`/path/to/shatter/shatter-rust-runtime`), and lists where it looked (the env var value if set but invalid, and the executable-ancestor walk).
- [ ] The failure-impact table has a `rust` row for Rust runtime-setup failures (a dedicated category such as `runtime_crate_missing`, added alongside the Go/TS classifiers at explore.rs:3244-3258). The `any` rollup row may remain.
- [ ] In machine mode (`--progress`) the once-per-run error follows the machine-mode stderr contract from scan-progress-post-hoc if that has landed; otherwise it is the same human line.
- [ ] `SHATTER_RUNTIME_PATH` is documented in the README install section and in the env-var table that str-qwua7.20.2 adds (if it exists by then), until str-qwua7.60 embeds the runtime.
- [ ] **Relocated-frontend integration test** (must fail on current main and pass after the fix): build or copy `shatter-rust` into a temp directory with no `shatter-rust-runtime` sibling in any of its five ancestors, put that directory first on PATH, **remove `SHATTER_RUNTIME_PATH` from the child environment**, run `shatter explore` from a temp cwd outside the repository on a `.rs` fixture with **at least three functions**, and assert: exactly one occurrence of the runtime-crate error on stderr, the message contains `SHATTER_RUNTIME_PATH`, and the failure-impact table has a `rust` row. On current main the error appears three times and there is no `rust` row.
- [ ] Positive control in the same test setup: with `SHATTER_RUNTIME_PATH` set to the real crate, explore of the same fixture completes with at least one executed path per function. This proves the documented remedy works.
- [ ] Record the red run on main and the green run on the branch in the close reason. `cargo test --test e2e_concolic_rust` passes. SPEC §8 has a changelog row. `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Make the runtime-crate check a per-run precondition on the Rust frontend session (check once when the session starts, or cache the first `FileNotFound` for the run) instead of a per-execute failure. Carry the language on the failure summary so the rollup can add a Rust row.

## Out of scope

- Embedding the Rust frontend or runtime in the shatter binary (str-qwua7.60).
- Doctor checks (str-qwua7.40, doctor-execution-readiness).
- The missing-frontend hint text and its duplication (str-qwua7.13).
- The general SHATTER_* env-var table (str-qwua7.20.2). This issue only adds the `SHATTER_RUNTIME_PATH` row, if that table exists by then.

## Related

str-qwua7.40 (open, doctor Rust section), str-qwua7.13 (open, hint once / scan-run agreement), str-qwua7.20.2, str-qwua7.60. In this bucket: doctor-rust-runtime-note, rust-hint-once-note, doctor-execution-readiness, scan-progress-post-hoc.

## Priority / Type

P2, bug.
