---
slug: rust-main-default-exclusion
kind: new
title: "Rust frontend offers `fn main` as an explore/scan target; Go already excludes its `main` entrypoint"
priority: P3
type: bug
labels: [rust-frontend, parity, discovery, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Rust frontend offers `fn main` as an explore/scan target; Go already excludes its `main` entrypoint

## Problem

Exploring `rust/04_errors.rs` explores `main` as a third target alongside `safe_divide` and `compute_stats`. A binary entrypoint takes no inputs, and running it executes the program's side effects rather than exercising a function under test. The Go frontend already excludes the literal `main` entrypoint for this reason (str-jeen.55); the Rust frontend has no equivalent. Split out of per-language-outcome-rendering during the cross-check.

## Evidence

Verified against the audit worktree (code at HEAD 56c86168):

- `audits/2026-09-22/cli-ux-transcripts/rust-explore3.err` lines 1-4: `[progress] starting 3/3: main` for `04_errors.rs`, a file with `safe_divide`, `compute_stats` and `main`.
- Go model: `shatter-go/protocol/analyzer.go:465-490` `isMainEntrypointDecl` matches only the literal free function `main` with no params and no results in `package main`, so helper functions in the same package stay discoverable.
- TS model for name-based exclusion: `shatter-core/src/discovery.rs:605-617` (`LIFECYCLE_EXPORT_NAMES`, str-qwua7.56).
- `grep -n '"main"' shatter-rust/src/analyzer.rs` finds only the `#[tokio::main]` attribute check at line 357, no entrypoint exclusion.
- This draft's claim was not independently re-verified by the audit verifier (cli-ux-11 note). The first acceptance item makes the implementer confirm it.

## Acceptance criteria

- [ ] Reproduce first: a test that analyzes a `.rs` fixture containing `fn main()` plus two ordinary functions lists `main` as a default target on current main. If it does not, close this issue with that test output as the reason.
- [ ] The Rust analyzer excludes a free `fn main()` with no parameters (including `#[tokio::main] async fn main()`) at the crate root of a binary target from default explore/scan target lists, mirroring Go's `isMainEntrypointDecl`. A function named `main` inside a module, an `impl` block, or with parameters stays discoverable.
- [ ] Naming it explicitly (`shatter explore file.rs:main`) still explores it, or fails with a clear message that says it is excluded as an entrypoint; pick one and document it in `shatter-rust/CLAUDE.md`.
- [ ] Tests: the fixture above (red on main, green after), plus negative cases for a `mod x { pub fn main() }` and `impl T { fn main(&self) }`.
- [ ] The change alters `analyze` output, so update `protocol/parity-matrix.yaml` (entrypoint exclusion per frontend) and `shatter-rust/CLAUDE.md`, then run `task parity` and `task conformance`.
- [ ] `cargo test --test e2e_concolic_rust` passes. `task affected` passes, with `Gates selected` recorded.

## Out of scope

- Outcome rendering (per-language-outcome-rendering).

## Related

str-jeen.55 (Go main exclusion), str-qwua7.56 (TS lifecycle exclusion). In this bucket: per-language-outcome-rendering.

## Priority / Type

P3, bug (noise in reports; no wrong results beyond an extra target).
