---
slug: core-reachability-gate
kind: new
title: "Add a production-reachability check for shatter-core pub items (defined roots and graph traversal), wired into check-static"
priority: P3
type: task
labels: [audit-2026-09-22, quality-gates, shatter-core, tech-debt]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [core-dead-code-removal]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Add a production-reachability check for shatter-core pub items (defined roots and graph traversal), wired into check-static

## Problem

`pub` items in a library crate never trigger rustc's dead-code warning, and cargo-machete covers only dependencies. That is how about 3,400 lines of shatter-core became production-dead without anyone noticing (core-dead-code-removal). A check is needed so it does not recur.

A simple "referenced from outside its own file" count is not enough:

- Two dead modules that reference each other both pass (for example `reporter.rs` and `clustering.rs` today).
- A live pub helper that is only called from within its own file, by a function that is itself reachable, fails.

This was part of the combined draft core-dead-code-removal and is split out as a separate deliverable.

## Acceptance criteria

- [ ] The check's model is written in the script header:
  - **Roots:** the `main` functions of the workspace binaries (`shatter-cli` and any other `[[bin]]`), plus pub items used by non-test code of other workspace crates (the frontends, shatter-llm). Test code, `tests/`, `benches/` and examples are not roots.
  - **Traversal:** reachability over a reference graph of items (functions, methods, types, modules), built from rustdoc JSON (`cargo +nightly rustdoc --output-format json`) or an equivalent item-level source.
  - **Report:** every pub module and pub fn in shatter-core not reachable from a root.
- [ ] If an item-level graph is not practical, the script may use a documented heuristic instead. It must then carry regression cases for both limits above (mutually referencing dead modules; a live helper used only in its own file) and state in the header which of them it gets wrong.
- [ ] An allowlist file with one reason per entry. The check fails on any new unreachable item not in the allowlist, and on any allowlisted item that has become reachable (stale entry).
- [ ] The script is wired into `task check-static`.
- [ ] Regression fixtures (a small test crate or synthetic graph input) cover:
  - an unreachable pub fn (reported);
  - two mutually referencing unreachable modules (both reported);
  - a pub helper called only within its own file from a reachable fn (not reported);
  - a pub fn used only by tests (reported).
- [ ] Proof at close: the regression fixture output; the script run directly (not through the cached task) on a branch with a planted unused pub fn in shatter-core (fails, naming it); the script passing on the final branch; the allowlist contents.

## Out of scope

- Deleting the currently dead code (core-dead-code-removal, which lands first so the initial allowlist is small).
- Unused-code tooling in the bento audit skill (bento-r85d).
- Module-cycle checks (str-qwua7.29).

## Metadata

- Priority: P3
- Type: task
- Labels: audit-2026-09-22, quality-gates, shatter-core, tech-debt
- Parent epic: Epic: Audit 2026-09-22 findings
- Blocked by: core-dead-code-removal
- Related: str-qwua7.29, bento-r85d, engine-parity-e2e
- Source findings: core-08 (split from draft shatter-code/18)
