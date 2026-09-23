---
slug: sandbox-backend-disables-guard
kind: new
title: "SHATTER_SANDBOX_BACKEND turns off the host-write guard for TS and Rust targets, and the docs recommend it"
priority: P1
type: bug
labels: [sandbox, safety, docs, cli, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# SHATTER_SANDBOX_BACKEND turns off the host-write guard for TS and Rust targets, and the docs recommend it

## Problem

When there is no sandbox, Shatter refuses to execute targets (str-gg9v). The documented remedy for that refusal is to set `SHATTER_SANDBOX_BACKEND=docker|bwrap`, and the docs call it the recommended one. Only the Go frontend reads this variable and implements a backend.

The CLI treats *any* non-empty value other than `none` as proof of confinement, for every frontend. When the variable is set it:

- passes the default-deny gate, and
- skips the throwaway-directory `IsolationGuard`, so `SHATTER_HOST_WRITE_DIR` is never exported.

For TypeScript and Rust targets this removes all write protection. Target code writes straight into the invoking directory. That is the incident str-gg9v was created to prevent, and the recommended remedy brings it back.

A secondary gap: the CLI accepts values the Go runner rejects, such as a typo like `dcoker`. The Go runner errors with `sandbox: unsupported backend`, but the CLI still counts the value as a sandbox and drops the guard.

## Evidence

Re-verified against the audit worktree (HEAD 56c86168):

- `shatter-cli/src/host_writes.rs:55-63` `sandbox_backend_configured()` returns true for any trimmed, non-empty value other than `none`.
- `shatter-cli/src/host_writes.rs:77-79` `execution_permitted()` passes the gate when that function returns true.
- `shatter-cli/src/host_writes.rs:142-145` `setup()` returns `Ok(None)` (no `IsolationGuard`) whenever a backend is "configured". The comment says "The OS sandbox already contains the target's writes."
- The only reader of the variable in the Rust, Go and TS sources is `shatter-go/sandbox/runner.go:16` (`EnvironmentBackendKey`). `runner.go:118` rejects unknown values.
- The TS and Rust frontends redirect relative writes only through `SHATTER_HOST_WRITE_DIR` (`shatter-rust/src/executor.rs:1045-1060`, and the TS executor, see `shatter-ts/src/executor.test.ts:753`). The CLI never sets that variable when a backend is set.
- Docs that recommend the variable with no Go-only caveat:
  - `README.md:309-310`: "Recommended: run targets inside an OS sandbox (Go frontend)". The Go-only fact sits in a comment while the line is labelled Recommended.
  - `README.md:321-322` and `README.md:326-329` tell CI and wrapper users to prefer `SHATTER_SANDBOX_BACKEND`.
  - `QUICKSTART.md:83-85` recommends it for the TS example.
  - `SPEC.md:606-609` and `SPEC.md:622-624` say a configured backend "satisfies both controls at once".
  - `refusal_message()` at `shatter-cli/src/host_writes.rs:98-115` says "Configure an OS sandbox (recommended)".
  - The `--allow-host-writes` help at `shatter-cli/src/args.rs:170-171` calls the backend "the safer alternative".
- Repro from the audit, on main 9516036d. A TS target `touch()` calls `fs.writeFileSync('marker-' + s + '.txt', ...)`:
  - `SHATTER_SANDBOX_BACKEND=docker shatter explore w.ts:touch --max-iterations 5` exits 0 and leaves `marker-long.txt` and `marker-short.txt` in the cwd.
  - The same run with `--allow-host-writes` instead leaves the cwd clean.
- Source findings: audit 2026-09-22 docs-01 (verdict confirmed, P1). Area evidence: `audits/2026-09-22/areas/docs.md`.

## Acceptance criteria

- [ ] For TS and Rust targets, a set `SHATTER_SANDBOX_BACKEND` never skips the `IsolationGuard`. Those frontends still get `SHATTER_HOST_WRITE_DIR`. The run prints one warning line: `SHATTER_SANDBOX_BACKEND applies to Go targets only; TS/Rust targets run in a throwaway directory`.
- [ ] Mixed-language runs (`scan`/`run` over a directory with TS, Go and Rust files) keep the guard for the non-Go frontends. Go keeps using its backend. Go already ignores `SHATTER_HOST_WRITE_DIR` when `Runner.Enabled()` is true (`shatter-go/CLAUDE.md` host-write paragraph).
- [ ] A backend value other than `none`, `bwrap` or `docker` is rejected up front with a clear error. It is never treated as a sandbox.
- [ ] Decide explicitly whether a TS- or Rust-only execution with only `SHATTER_SANDBOX_BACKEND` set passes the default-deny gate, and record the decision in SPEC §2.10. Recommended: it passes, is equivalent to `--allow-host-writes`, and prints the warning above.
- [ ] Regression tests, one per frontend (TS, Go, Rust), each run as a CLI integration test from a temp cwd. A target that writes a relative file, run under each opt-in (`--allow-host-writes`, `SHATTER_ALLOW_HOST_WRITES=1`, and `SHATTER_SANDBOX_BACKEND=docker`), leaves the cwd unchanged. For Go, where docker or bwrap is not available on the runner, the test may use a stub backend or skip with a logged reason. The TS and Rust `SHATTER_SANDBOX_BACKEND` cases must **fail on current main and pass after the fix**. Record both runs in the close reason.
- [ ] README ("Executing Target Functions Safely" and the CI paragraph), QUICKSTART, SPEC §2.10, the `refusal_message()` text and the `--allow-host-writes` help state that OS sandbox backends are Go-only and recommend `--allow-host-writes` or `SHATTER_ALLOW_HOST_WRITES=1` for TS and Rust.
- [ ] SPEC §8 has a changelog row for the behavior change.
- [ ] `protocol/parity-matrix.yaml` records sandbox-backend support per frontend (Go yes, TS no, Rust no), and `task parity` passes.
- [ ] `task affected` passes, with its `Gates selected` output recorded in the close reason.

## Suggested approach

Make the "is this contained?" decision per frontend, not per process:

1. Always create the `IsolationGuard` for execution commands, whether or not a backend is set, and export `SHATTER_HOST_WRITE_DIR`. Go already ignores it when its sandbox is enabled, so the TS and Rust redirect comes back without any Go change. This is the smallest correct change and removes the early `return Ok(None)` at `host_writes.rs:142-145`.
2. Validate the backend value in `sandbox_backend_configured()` against the set Go accepts.
3. Drive the per-frontend rule from data: add a `sandbox_backends` capability row to `protocol/parity-matrix.yaml` so a future TS or Rust backend is a matrix change plus implementation, not a hidden CLI assumption.

## Out of scope

- Implementing OS sandbox backends for TS or Rust.
- Redirecting relative writes under `--allow-host-writes` for TS paths not yet covered. That is str-joyqu (open).
- The SPEC changelog backfill errors from str-qwua7.8. That is spec-changelog-backfill.

## Related

str-gg9v (original default-deny), str-02i70 (per-frontend throwaway-dir redirect), str-joyqu, str-qwua7.8 (closed; its addendum criterion required the docs never to treat the Go-only variable as proof of confinement), sa-oio (shatter-agents note that the backend is Go-only). The comment on str-qwua7.8 is docs-first-run-reopen-note.

## Priority / Type

P1, bug. This is a safety regression: the documented recommended remedy lets target code write into the user's repository.
