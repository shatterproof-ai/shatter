---
slug: rust-runtime-path-and-doctor
kind: new
title: "Following the Rust-frontend hint still fails on the undocumented SHATTER_RUNTIME_PATH, and `shatter doctor` reports all green"
priority: P2
type: bug
labels: [rust-frontend, docs, ux, install, doctor, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Following the Rust-frontend hint still fails on the undocumented SHATTER_RUNTIME_PATH, and `shatter doctor` reports all green

## Problem

A user who explores a `.rs` target outside the shatter source tree hits a chain of failures, and nothing in the tooling explains it:

1. **Without `shatter-rust`**, explore prints a roughly 600-character, contributor-oriented hint twice (1,345 bytes of stderr). It frames the situation as "the expected state after `cargo build --release --bin shatter` from the workspace root", which is source-checkout guidance shown to every user.
2. **With `shatter-rust` on PATH**, as the hint instructs, every function fails with `execute error (FileNotFound): cannot locate shatter-rust-runtime crate; set SHATTER_RUNTIME_PATH`. The error repeats once per function, and the "Failure impact" table labels the language `any` although the target is `.rs`.
3. **`SHATTER_RUNTIME_PATH` is undocumented.** It does not appear in README, QUICKSTART, SPEC or any `--help` text.
4. **`shatter doctor` exits 0** and checks none of this: not the Rust frontend, the runtime crate, the node or go toolchains, or sandbox and host-write readiness. Without sandbox or host-write readiness, every execution command refuses to run.

## Evidence

Re-verified against the audit worktree (HEAD 56c86168):

- The hint text is `RUST_FRONTEND_INSTALL_HINT` at `shatter-cli/src/helpers.rs:418-424` (the audit cited line 497, which is stale).
- The runtime lookup and the error are at `shatter-rust/src/executor.rs:1200-1223`. `SHATTER_RUNTIME_PATH` is read first, and the fallback error is `"cannot locate shatter-rust-runtime crate; set SHATTER_RUNTIME_PATH"`.
- `shatter-cli/src/commands/build_frontend.rs:607-630` is the only other place that mentions the variable (warnings during `build-frontend`).
- `shatter-cli/src/commands/doctor.rs:59-61` runs `check_embedded_frontend` and `check_generated_paths_ignored` plus the project-configuration report. There are no toolchain, runtime-crate or sandbox checks.
- `grep -n SHATTER_RUNTIME_PATH README.md QUICKSTART.md SPEC.md` returns no matches.
- Transcripts in `audits/2026-09-22/cli-ux-transcripts/`:
  - `rust-explore.err` (1.3 KB) has the hint printed twice.
  - `rust-explore2.err` has the runtime-crate error three times, plus the `any` language label.
  - `doctor.out` shows version and hashes, the project configuration, "Embedded Go frontend: up to date." and "Generated-path gitignore: all configured output paths are ignored." It exits 0.
- Root cause of the missing test coverage: the E2E Rust suite (`shatter-core/tests/e2e_concolic_rust.rs`) runs from the workspace root, where the runtime crate is found automatically, so the out-of-tree path is never exercised.
- Source findings: audit 2026-09-22 cli-ux-10 (confirmed, P2). Area evidence: `audits/2026-09-22/areas/cli-ux.md` F10.

## Acceptance criteria

- [ ] `shatter doctor` reports each of the following with a pass/warn/fail status and a one-line fix command:
  - Rust frontend resolution: which `shatter-rust` is found, or why none is.
  - shatter-rust-runtime crate location: `SHATTER_RUNTIME_PATH` value, or the auto-discovered path, or "not found".
  - node and go toolchain presence and version.
  - Sandbox and host-write readiness: whether a backend or `SHATTER_ALLOW_HOST_WRITES` is set, and what execution commands will do.
- [ ] `doctor` exits non-zero when a check that blocks execution fails. It stays exit 0 for warnings, such as "Rust frontend absent; Rust targets will be skipped". The exit codes are documented in SPEC §2.9.
- [ ] The missing-frontend hint is at most two lines, printed once per run, and points at `shatter doctor` for details. Source-checkout build instructions move to README "Build from source".
- [ ] The runtime-crate failure is reported once per run, not once per function, and names `SHATTER_RUNTIME_PATH` with an example value. The failure-impact table shows `rust` for `.rs` targets.
- [ ] `SHATTER_RUNTIME_PATH` is documented in the README install section and in the env-var table that str-qwua7.20.2 adds, until str-qwua7.60 embeds the runtime.
- [ ] A test runs a Rust explore from a temp directory outside the repo with `SHATTER_RUNTIME_PATH` unset, and asserts either success or the single actionable error. A second test asserts `doctor`'s runtime-crate check output in the same setup. Both must fail on current main (repeated error or all-green doctor) and pass after the fix. Record both in the close reason.
- [ ] SPEC §2.9 and §8 are updated. `task affected` passes, with `Gates selected` recorded, and `cargo test --test e2e_concolic_rust` passes.

## Suggested approach

Add doctor checks as small, independent functions that follow the existing `check_*` pattern in `doctor.rs`. Reuse the frontend-resolution helper in `helpers.rs` so doctor and explore agree on what "found" means. Reuse `sandbox_backend_configured()` and `execution_permitted()` from `host_writes.rs` for the readiness line. Once sandbox-backend-disables-guard lands, report the backend as Go-only.

## Out of scope

- Embedding the Rust frontend or runtime in the shatter binary (str-qwua7.60).
- The general SHATTER_* env-var table (str-qwua7.20.2). This issue only adds the `SHATTER_RUNTIME_PATH` row, if that table exists by then.

## Related

str-qwua7.40 (open; extends doctor to Rust-frontend resolution. Coordinate with it or fold it in), str-qwua7.20.2, str-qwua7.60, str-qwua7.13 (prints the hint once), sandbox-backend-disables-guard (the readiness line should reflect its Go-only rule).

## Priority / Type

P2, bug.
