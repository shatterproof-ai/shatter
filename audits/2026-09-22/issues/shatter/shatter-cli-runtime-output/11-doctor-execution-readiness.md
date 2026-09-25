---
slug: doctor-execution-readiness
kind: new
title: "`shatter doctor` does not check node/go toolchains or sandbox/host-write readiness, so it reports all green on a machine where every execution command refuses to run"
priority: P2
type: feature
labels: [doctor, ux, install, sandbox, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [sandbox-backend-disables-guard]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# `shatter doctor` does not check node/go toolchains or sandbox/host-write readiness

## Problem

`shatter doctor` exited 0 in the audit on a machine where no execution command would run: no sandbox backend and no `SHATTER_ALLOW_HOST_WRITES`, so default-deny (str-gg9v) refuses every `explore`/`scan`/`run`. It also says nothing about whether the node and go toolchains that the TS and Go frontends need are present. Split out of rust-runtime-path-and-doctor during the cross-check; the Rust frontend and runtime-crate checks belong to str-qwua7.40 (see doctor-rust-runtime-note).

## Evidence

Re-verified against the audit worktree (code at HEAD 56c86168):

- `shatter-cli/src/commands/doctor.rs:40-63`: doctor prints version and hashes, the project configuration report, `check_embedded_frontend` and `check_generated_paths_ignored`, and returns `Ok(frontend_ok && gitignore_ok)`. There are no toolchain or host-write checks.
- `shatter-cli/src/host_writes.rs:55-79`: `sandbox_backend_configured()` and `execution_permitted()` decide the default-deny gate; doctor never calls them.
- Transcript `audits/2026-09-22/cli-ux-transcripts/doctor.out`: version and hashes, project configuration, "Embedded Go frontend: up to date.", "Generated-path gitignore: all configured output paths are ignored.", exit 0.
- Source findings: audit 2026-09-22 cli-ux-10 (confirmed, P2). Area evidence: `audits/2026-09-22/areas/cli-ux.md` F10.

## Severity contract (defined here so a TS-only install never fails on unused prerequisites)

A language is **in use** when the resolved project root contains source files of that language that a scan would select (reuse `list-targets` selection, respecting config excludes) or the project config names it. A language is **required** only when the user passes the require flag that str-qwua7.13 settles on (`--require-frontend <lang>` is its proposal).

| Check | Not in use | In use | Required |
|---|---|---|---|
| node (TS) / go (Go) toolchain missing or below the minimum version | info line, no warning | warn | fail |
| Host-write readiness: neither a valid backend nor `SHATTER_ALLOW_HOST_WRITES`/`--allow-host-writes` available | — | warn ("execution commands will refuse to run; set ...") | warn |
| `SHATTER_SANDBOX_BACKEND` set to a value outside `none`/`bwrap`/`docker` | fail | fail | fail |

Default-deny with no opt-in is a safe, intended state, so it is never a failure. Existing failing checks (embedded frontend staleness, un-ignored generated paths) keep their current severity.

## Acceptance criteria

- [ ] Doctor prints a toolchain line for node and go (found path and version, or "not found"), with the severity from the table and a one-line fix command.
- [ ] Doctor prints a host-write readiness line that states what execution will do per language, using the per-frontend rule from sandbox-backend-disables-guard (for example "Go: runs under docker; TS/Rust: refused unless --allow-host-writes"). It calls the same helpers as the execution path, so doctor and execution cannot disagree.
- [ ] Exit status: non-zero iff any check is `fail` per the table. The contract is written into SPEC §2.9 with a §8 changelog row.
- [ ] Tests (CLI integration, temp project dirs, `PATH` controlled so node/go presence is deterministic):
  - TS-only project, go absent from PATH, no host-write opt-in: exit 0; go line is info, host-write line is warn.
  - TS-only project, node absent from PATH: exit 0, node line is warn.
  - Same, with the require flag for `ts`: non-zero exit.
  - `SHATTER_SANDBOX_BACKEND=dcoker`: non-zero exit, message names the accepted values.
  - The TS-project-with-no-opt-in case shows no host-write line on current main (all green), so it fails on main and passes after the fix. Record red and green runs in the close reason.
- [ ] `task affected` passes, with `Gates selected` recorded.

## Suggested approach

Add `check_toolchains` and `check_host_write_readiness` next to the existing `check_*` functions in `doctor.rs`, each returning a pass/warn/fail/info status that the caller folds into the exit code. Reuse `list-targets` language detection and the `host_writes.rs` helpers.

## Out of scope

- The Rust frontend and runtime-crate section (str-qwua7.40, with doctor-rust-runtime-note).
- Choosing the require-flag spelling (str-qwua7.13).

## Related

str-qwua7.40, str-qwua7.13, str-gg9v. In this bucket: sandbox-backend-disables-guard (blocks this: the readiness line must report its per-frontend rule), rust-runtime-path-and-doctor, doctor-rust-runtime-note.

## Priority / Type

P2, feature.
