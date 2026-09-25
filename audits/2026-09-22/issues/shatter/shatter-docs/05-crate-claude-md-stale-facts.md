---
slug: crate-claude-md-stale-facts
kind: new
title: "Correct stale and false facts in the crate CLAUDE.md files (core, ts, go, rust) and the parity-matrix notes they cite"
priority: P2
type: task
labels: [docs, agents, parity, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Correct stale and false facts in the crate CLAUDE.md files (core, ts, go, rust) and the parity-matrix notes they cite

## Problem

Claude Code injects the crate CLAUDE.md files into any agent that reads a file in that subtree, and the root CLAUDE.md tells dispatchers to rely on this. Each of the four files contains claims that are now false. Those claims send agents to the wrong engine, to dead code, or to a contract the code does not follow. The items below are the ones not already owned by the larger restructuring issues (str-qwua7.24 generated tables, str-qwua7.25 slimming), which have had no commits since 2026-09-04. Items those issues already own were removed from this draft; str-qwua7.25 gets a refreshed-evidence note instead (qwua7-25-refresh-note).

## Evidence

All lines were re-verified at 56c86168. Where this issue replaces a line-number reference, use the symbol name instead.

### shatter-core/CLAUDE.md
- `:7` reads "`explorer.rs` — Concolic exploration loop". explorer.rs is the random/hybrid engine. The concolic engine is `orchestrator.rs`, which `:17` describes only as "Multi-round exploration orchestration".
- `:3` ("export logic") and `:11` (`export.rs`) describe dead code. str-qwua7.59 deletes it. If .59 lands first, drop these lines there; otherwise drop them here.
- The Key Modules list has no entry for `solver.rs`, `strategy.rs`, `shrink.rs`, `pipeline_orchestrator.rs` or `planner_consumer.rs`.

### shatter-ts/CLAUDE.md
- `:375` says "str-jeen.40 will refine bucketing". str-jeen.40 is closed.
- The Key Files list names 2 of about 30 `src/` modules.
- **Not in this issue (already owned elsewhere):** the stale line ranges at `:16-17` and the `.js` paths at `:379-380` are str-qwua7.25's acceptance ("Line-number citations replaced by symbol names; the two .js references corrected"); the refreshed evidence goes to that issue as note `qwua7-25-refresh-note`. The "TS is the only frontend that produces `ite`" claim at `:44` is str-qwua7.24's acceptance ("The five stale claims above no longer appear").

### shatter-go/CLAUDE.md (54,640 bytes)
- `:57-58` name `instrument/flow.go` and `instrument/flowwalk.go` as the `ite` mechanism. `deadcode ./...` reports both as unreachable. The live path is `walkBodyForFlow` → `buildSymExprWithFlow` (see matrix `ite-symexpr-production-partial`).
- `:273` says `planner.ResolveMockSpecs` emits MockSpecs. `deadcode` reports it as unreachable.
- `str-8v66` and `str-ruw0` are cited in CLAUDE.md and in `shatter-go/planner/plan.go:100,128`. `bd show` returns "no issue found" for both.
- `:318` mentions `SHATTER_HARNESS_CACHE`, but no Go source reads that variable.
- `:138` reads "TS and Rust currently declare `outcome` only". This is stale (protocol-parity-13). str-qwua7.24 fixes the same claim in the TS and Rust files only; this Go-file sentence is not in its list. For example, TS's handshake `SUPPORTED_CAPABILITIES` (`shatter-ts/src/handlers.ts:56`ff.) declares analyze, execute, instrument, prepare, setup, teardown, generate and many `complex_type:*` entries. Restate which *Go-only* capabilities TS and Rust decline, and point to the matrix.
- The outcome status list omits `preflight_failed`. `:240` mentions `skipped_by_policy` separately, so check whether the list itself includes it.

### shatter-rust/CLAUDE.md, plus the matrix entries it points to
- `:109` and `protocol/parity-matrix.yaml:887` (`adapter_capabilities.async_runtime`) say `tokio::runtime::Runtime::new().block_on(...)`. The generated harness uses `tokio::runtime::Builder::new_current_thread()` at `shatter-rust/src/executor.rs:2547, 2845, 4942, 6801`, and does so deliberately.
- `:79` and `:93` cite a "single-file constraint". `shatter-rust/src/analyzer.rs:834-952` resolves same-crate cross-file types, and CLAUDE.md `:86` itself describes a cross-file enum E2E.
- (`:234-235` stale `handler.rs` line numbers are str-qwua7.25's line-number item; see note `qwua7-25-refresh-note`.)
- `protocol/parity-matrix.yaml:496` marks `rust: captured` for `console_output`. The same entry's notes (`:491-492`) and `protocol/PARITY.md:120` say the crate-bridge harness does not capture it, and `PARITY.md:109` still shows ✅. Mark it partial, with the crate-bridge exception.
- The matrix's axum adapter note (around `:897-907`) does not list the `Multipart` extractor, which the adapter handles.

### Dropped from the original draft
The claim that "PARITY.md:96 says 30 s but the Rust timeout fallback is 120 s" did not re-verify. `executor.rs:1017` `DEFAULT_BUILD_TIMEOUT_SECS = 120` is a build timeout. `PARITY.md:133-135` lists Rust's execution timeout default as 5 s, and "30" appears nowhere in PARITY.md.

## Acceptance criteria

- [ ] Every item under Evidence is corrected, or deleted in favour of a pointer to `protocol/parity-matrix.yaml`/`PARITY.md`. The close note lists each item with its disposition.
- [ ] Every line this issue edits refers to code by symbol name, not source line number. Removing the remaining line-number citations across the three frontend files is str-qwua7.25; do not expand into it here.
- [ ] `task parity` passes after the matrix edits (console_output partial, async_runtime flavour, axum Multipart). Run the validator directly and paste the output, because `task` results can be served from the checksum cache.
- [ ] A check fails when a crate CLAUDE.md cites a `src/…` path that does not exist, or a `str-*` ID that the tracker cannot resolve. Its home is the agent-rules drift lint, str-u394l.4, or a `task docs` step if .4 is not ready.
  - Proof at close: a unit test that fails on a fixture CLAUDE.md citing a nonexistent `src/…` path and `str-8v66`, and passes on the committed crate CLAUDE.md files (if str-qwua7.25 has not yet fixed the `.js` paths, the real files will still fail; land after .25's TS PR or allowlist those two paths with a pointer to .25).
  - Divergence-ID resolution is str-qwua7.34's check; do not duplicate it.

## Suggested approach

Work file by file. Verify each claim against `rg`/`deadcode`/`bd show`, then correct it or replace it with a pointer. Keep the edits minimal so they do not conflict with str-qwua7.25's later slimming.

## Out of scope

- Generating capability tables from the matrix (str-qwua7.24).
- Slimming the files to 5 KB, frontend line-number citations and the two `.js` paths (str-qwua7.25).
- The TS `ite` and TS/Rust "outcome only" claims (str-qwua7.24).
- The false "does not emit `preflight_failed`" claims (shatter-go/CLAUDE.md:206, `shatter-go/protocol/constants.go:16-19`, `types.go:584-590`) and the dangling divergence IDs (`loop-body-states-typescript-only`, `error-code-preflight-failed-typescript-only`, `rust-side-effects-not-captured`). These are already str-qwua7.34's acceptance; coordinate so they are not edited twice.
- Adding `.rs` to the `explore --help` extension list (`shatter-cli/src/args.rs:501`). That is a CLI help change, not a CLAUDE.md fact.

## Dependencies

- Blocked by: none.
- Related: str-qwua7.24, str-qwua7.25 (see note qwua7-25-refresh-note), str-qwua7.34, str-qwua7.59, str-u394l.4.

## Source

Audit 2026-09-22, findings core-21, frontend-ts-15, frontend-go-09, frontend-rust-08 and protocol-parity-13 (docs-24 goes to str-qwua7.25 via qwua7-25-refresh-note) (all confirmed). Draft `shatter-docs-ui/22`. Evidence is in `audits/2026-09-22/areas/{core-engine,frontend-ts,frontend-go,frontend-rust,protocol-parity,docs}.md` (on branch `audit-2026-09-22` until the audit directory lands on `main`).
