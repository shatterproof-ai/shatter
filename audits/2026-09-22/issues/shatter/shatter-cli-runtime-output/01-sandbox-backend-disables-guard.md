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
- Docs that present the variable as the recommended remedy. Only `README.md:309` carries a Go-only note, and it is a parenthetical on a line labelled Recommended; every other site has none:
  - `README.md:309-310`: "# Recommended: run targets inside an OS sandbox (Go frontend)." followed by `export SHATTER_SANDBOX_BACKEND=docker`. It does not say that TS and Rust targets lose write protection.
  - `README.md:321-322` and `README.md:326-329` tell CI and wrapper users to prefer `SHATTER_SANDBOX_BACKEND`.
  - `QUICKSTART.md:83-85` recommends it for the TS example.
  - `SPEC.md:606-609` and `SPEC.md:622-624` say a configured backend "satisfies both controls at once".
  - `refusal_message()` at `shatter-cli/src/host_writes.rs:98-115` says "Configure an OS sandbox (recommended)".
  - The `--allow-host-writes` help at `shatter-cli/src/args.rs:170-171` calls the backend "the safer alternative".
  - `shatter-go/CLAUDE.md:257` says "the CLI never sets `SHATTER_HOST_WRITE_DIR` in that case". The fix below makes that sentence false.
- Repro from the audit, on main 9516036d. A TS target `touch()` calls `fs.writeFileSync('marker-' + s + '.txt', ...)`:
  - `SHATTER_SANDBOX_BACKEND=docker shatter explore w.ts:touch --max-iterations 5` exits 0 and leaves `marker-long.txt` and `marker-short.txt` in the cwd.
  - The same run with `--allow-host-writes` instead leaves the cwd clean.
- Source findings: audit 2026-09-22 docs-01 (verdict confirmed, P1). Area evidence: `audits/2026-09-22/areas/docs.md`.

## Execution policy (decided in this draft; the maintainer can override before filing)

`SHATTER_SANDBOX_BACKEND` confines only Go targets. For TS and Rust targets it is **not** an opt-in to execution. This is fail-closed, the same as the str-gg9v default-deny: a user who asked for an OS sandbox and did not get one should be refused, not silently downgraded to unsandboxed execution.

| Opt-ins present | Go targets | TS / Rust targets |
|---|---|---|
| backend only | run under the backend | **refused** (default-deny) with a message that the backend is Go-only and names `--allow-host-writes` / `SHATTER_ALLOW_HOST_WRITES=1` |
| backend + `--allow-host-writes` (or `SHATTER_ALLOW_HOST_WRITES=1`) | run under the backend | run unsandboxed in the throwaway directory (`IsolationGuard`, `SHATTER_HOST_WRITE_DIR` exported) |
| `--allow-host-writes` only | run in the throwaway directory | run in the throwaway directory |

In a mixed-language `scan`/`run` with the backend only, the Go targets run and the TS/Rust targets are reported as skipped with the refusal reason (same status mechanism as other skipped targets); the command does not abort the Go work.

## Acceptance criteria

- [ ] The table above is implemented and recorded in SPEC §2.10, with a §8 changelog row for the behavior change.
- [ ] Whenever any TS or Rust target is executed, the `IsolationGuard` exists and `SHATTER_HOST_WRITE_DIR` is exported to that frontend, whether or not a backend is set. The early `return Ok(None)` at `host_writes.rs:142-145` is gone.
- [ ] A backend value other than `none`, `bwrap` or `docker` (after trimming, case as Go's `runner.go` accepts it) is rejected before any frontend is spawned, with an error naming the accepted values. It is never treated as a sandbox. Unit test covers `dcoker`, empty, `none`, `bwrap`, `docker`.
- [ ] CLI integration tests, each run from a fresh temp cwd with a target that writes a relative marker file **and** returns a value that proves it ran:
  - TS and Rust, `--allow-host-writes`: exit 0, the report shows at least one executed path for the target (paths > 0), the marker file exists inside the throwaway directory (the test locates it via a test-only hook or by pointing `SHATTER_HOST_WRITE_DIR`'s parent at a test-owned dir) and the cwd is byte-for-byte unchanged. A refused or failed run fails this test.
  - TS and Rust, backend only (`SHATTER_SANDBOX_BACKEND=docker`, no allow flag): the target is refused, the stderr names the Go-only rule, zero executions happen, and the cwd is unchanged. On current main this case exits 0 and leaves `marker-*.txt` in the cwd, so it **fails on current main and passes after the fix**.
  - TS and Rust, backend + `--allow-host-writes`: same assertions as the allow-only case (execution proven, cwd clean). Fails on current main (marker in cwd).
  - Mixed directory (TS + Go files), backend only, with a stub Go backend or a logged skip when neither docker nor bwrap exists: the TS target is reported as skipped with the refusal reason; the Go target is attempted.
  - Record the red run on main and the green run on the branch (test names and output excerpts) in the close reason.
- [ ] Warning and refusal text: in human mode each is one plain line on stderr. In machine mode (`--progress`) they follow the machine-mode stderr contract that scan-progress-post-hoc defines (a JSON log object whose `message` is the same text). If scan-progress-post-hoc has not landed, the human line is used in both modes.
- [ ] Docs state that OS sandbox backends are Go-only and that TS and Rust need `--allow-host-writes` or `SHATTER_ALLOW_HOST_WRITES=1`: README ("Executing Target Functions Safely" and the CI/wrapper paragraphs at 321-329), QUICKSTART 83-85, SPEC §2.10 (606-624), the `refusal_message()` text, the `--allow-host-writes` help, and `shatter-go/CLAUDE.md:257` (the "CLI never sets `SHATTER_HOST_WRITE_DIR`" sentence).
- [ ] `protocol/parity-matrix.yaml` records sandbox-backend support per frontend (Go yes, TS no, Rust no), and `task parity` passes.
- [ ] `task affected` passes, with its `Gates selected` output recorded in the close reason.

## Suggested approach

Make the "is this contained?" decision per frontend, not per process:

1. Always create the `IsolationGuard` for execution commands that will run a TS or Rust target, and export `SHATTER_HOST_WRITE_DIR`. Go already ignores it when its sandbox is enabled (`Runner.Enabled()`), so no Go change is needed.
2. Replace the process-wide `execution_permitted()` with a per-language check: Go passes with a valid backend or the allow opt-in; TS and Rust pass only with the allow opt-in.
3. Validate the backend value in `sandbox_backend_configured()` against the set Go accepts.
4. Drive the per-frontend rule from data: a `sandbox_backends` capability row in `protocol/parity-matrix.yaml`, so a future TS or Rust backend is a matrix change plus implementation, not a hidden CLI assumption.

## Out of scope

- Implementing OS sandbox backends for TS or Rust.
- Redirecting relative writes under `--allow-host-writes` for TS paths not yet covered. That is str-joyqu (open).
- The SPEC changelog backfill errors from str-qwua7.8. That is spec-changelog-backfill.

## Related

str-gg9v (original default-deny), str-02i70 (per-frontend throwaway-dir redirect), str-joyqu, str-qwua7.8 (closed; its addendum criterion required the docs never to treat the Go-only variable as proof of confinement), sa-oio (shatter-agents note that the backend is Go-only). The comment on str-qwua7.8 is docs-first-run-reopen-note. In this bucket, doctor-execution-readiness reports this rule and scan-progress-post-hoc defines the machine-mode stderr contract.

## Priority / Type

P1, bug. This is a safety regression: the documented recommended remedy lets target code write into the user's repository.
