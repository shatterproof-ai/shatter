---
slug: execute-request-builder
kind: new
title: "Build every concolic and random-explorer Execute request through one helper so sites stop re-deciding prepare_id/execution_profile/setup_context/capture"
priority: P3
type: task
labels: [concolic, explorer, orchestrator, refactor, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [concolic-refine-execute-builder, concolic-refine-path-accounting, str-qwua7.5]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Build every concolic and random-explorer Execute request through one helper so sites stop re-deciding prepare_id/execution_profile/setup_context/capture

## Problem

The two engines build `Command::Execute` inline at 14 sites (8 in `orchestrator.rs`, 6 in `explorer.rs`). Each site re-decides `prepare_id`, `execution_profile`, `setup_context` and `capture`. The audit found three bugs that come from sites drifting apart: the refine phase dropping `prepare_id`/`execution_profile` (concolic-refine-execute-builder), shrink sites passing `setup_context: None` (concolic-setup-teardown), and hard-coded `capture` values (str-qwua7.5). A single builder stops the next one.

This is a behaviour-preserving refactor. It lands after the field fixes, so it changes no request contents.

## Evidence

Line numbers were re-checked on the audit branch, whose code is identical to `56c86168`:

- `grep -c 'Command::Execute {'`: `shatter-core/src/orchestrator.rs` 8, `shatter-core/src/explorer.rs` 6. Other files also build Execute requests inline: `genetic_explorer.rs` 1, `observe.rs` 1, `revalidation.rs` 1, `recursive.rs` 4. `frontend.rs` (4) and `protocol.rs` (14) are the definitions and serde code, and `test_arbitraries.rs` (1) is a test generator.
- Orchestrator sites (by `capture:` literal): `:1694`, `:2380`, `:2701`, `:2714`, `:3103`, `:3643`, `:3687`, `:3741`. Explorer sites include the float-probe pair (`:1240-1270`) and the shrink sites (`:1778`, `:1823`, `:1875`).

## Scope

In scope: the Execute sites in `shatter-core/src/orchestrator.rs` and `shatter-core/src/explorer.rs`.

Out of scope, and not required to use the helper: `genetic_explorer.rs`, `observe.rs`, `revalidation.rs`, `recursive.rs`. The close note lists them as remaining inline sites. A follow-up may migrate them.

## Acceptance criteria

- [ ] One helper (for example `ExecuteRequestBuilder`, or a method on a small per-run context struct) builds the Execute request from the per-run values (`prepare_id`, `execution_profile`, `setup_context`, `plan`) plus per-call values (`function`, `inputs`, `mocks`, `capture`). `capture` is a parameter the caller passes, so str-qwua7.5's per-phase policy stays at the call sites.
- [ ] `grep -n 'Command::Execute {' shatter-core/src/orchestrator.rs shatter-core/src/explorer.rs` matches only inside the helper and inside `#[cfg(test)]` modules. The close note quotes the command and its output.
- [ ] No request contents change. Before the refactor, add a test with a recording frontend double that runs one random and one concolic exploration (with setup, a `prepare_id` and an `execution_profile` set) and snapshots the sequence of Execute requests. The same snapshot passes unchanged after the refactor. Quote both runs in the close note. If concolic-setup-teardown has not landed yet, the explorer shrink sites keep sending `setup_context: None` through an explicit, commented override on the helper, so this refactor does not quietly fix or change that behaviour.
- [ ] `task affected` (with `Gates selected` recorded) and `task e2e` pass.

## Suggested approach

Introduce the helper, then migrate sites one file at a time, running the snapshot test after each file.

## Out of scope

- Changing any field's value at any site. That belongs to concolic-refine-execute-builder, concolic-setup-teardown and str-qwua7.5.
- Session-lifecycle extraction (str-inct (b)) and shared-shrink extraction (str-qwua7.6 / str-qwua7.6.1).

## Priority

P3: a maintainability refactor. The user-visible bugs are fixed by the issues it depends on.

## Type

task

## Dependencies

- Blocked by: concolic-refine-execute-builder and concolic-refine-path-accounting (both edit the refine site). Also wait for open str-qwua7.5 to land, since it edits every orchestrator site's `capture` field; `blocked_by` only lists slugs in this bundle, so add str-qwua7.5 as a blocker when filing.
- Related: concolic-setup-teardown (its helper supplies `setup_context`), str-qwua7.6, str-qwua7.6.1, str-inct.

## References

Audit 2026-09-22 finding core-06, builder part, and core-16 (a duplicate of open str-qwua7.5, context only). Split from concolic-refine-execute-builder after the Codex cross-check and the same-runtime review, which found the earlier grep criterion could not pass because of Execute sites outside the two engine files.
