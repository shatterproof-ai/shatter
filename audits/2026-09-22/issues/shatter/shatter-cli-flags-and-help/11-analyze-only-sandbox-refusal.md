---
slug: analyze-only-sandbox-refusal
kind: new
title: "explore/run --analyze-only is refused by the host-write gate although it executes nothing"
priority: P3
type: bug
labels: [cli, sandbox, usability, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# explore/run --analyze-only is refused by the host-write gate although it executes nothing

## Problem

`shatter explore <file>:<fn> --analyze-only` only analyzes the function (it prints parameter and branch counts) and never executes a target. It is still refused with `Error: refusing to execute target functions without a sandbox.` (exit 2) when no sandbox backend is configured and `--allow-host-writes` / `SHATTER_ALLOW_HOST_WRITES` is not set. str-gg9v introduced the default-deny gate without an analyze-only exemption, so the cheapest first-use command demands a sandbox decision.

## Evidence

Re-verified against `audit-2026-09-22` (source at `56c86168`):

- `shatter-cli/src/main.rs:192` calls `host_writes::setup(&cli.command, cli.allow_host_writes)` before dispatch; on `Err` it prints the refusal and exits.
- `shatter-cli/src/host_writes.rs:84-95` `command_executes_targets()` matches `CliCommand::Explore(_)` and `CliCommand::Run { .. }` regardless of their `analyze_only` fields (`args.rs:546` for explore, `args.rs:1389` for run).
- findings.json goals-18, verifier verdict "confirmed": `explore 05-unions.ts:computeArea --analyze-only` prints the sandbox refusal and exits 2; with `--allow-host-writes` it prints `computeArea (05-unions.ts:17) params: 1, branches: 6`.
- The artifacts-16 verifier did not see the refusal in `audits/2026-09-22/artifact-samples/analyze-only-ts.err` or `cli-ux-transcripts/err-analyze-only.*`; those were captured with host writes allowed. Reproduce with `SHATTER_ALLOW_HOST_WRITES` and `SHATTER_SANDBOX_BACKEND` unset.

## Acceptance criteria

- [ ] A CLI test runs `shatter explore <ts-fixture>:<fn> --analyze-only` with `SHATTER_ALLOW_HOST_WRITES` and the sandbox-backend variable removed from the environment and no `--allow-host-writes`, and asserts exit 0 and the analysis line on stdout. The test fails on current HEAD with the refusal message; the close comment records both runs.
- [ ] The same test exists for `shatter run --analyze-only` (or the close comment shows `run --analyze-only` executes targets and must stay gated, with the code path cited).
- [ ] The exemption is decided from the parsed command, not from argv: `command_executes_targets()` (or its replacement from help-hides-execution-flags) returns false for explore/run when `analyze_only` is set, and a unit test covers both values of the flag.
- [ ] Without `--analyze-only`, explore still refuses as before; the existing host-write refusal tests pass unchanged.
- [ ] `task affected` passes. The close comment records the gates selected.

## Out of scope

- The content of the analyze-only output (analyze-only-output-detail).
- Scoping `--allow-host-writes` to executing commands (help-hides-execution-flags).

## Dependencies

- Blocked by: none.
- Related: str-gg9v (closed; introduced the gate), help-hides-execution-flags (same predicate), analyze-only-output-detail.

## Source

Audit 2026-09-22 finding goals-18 (P3), refusal half. Split from cli-minor-output-and-help-polish item 1.
