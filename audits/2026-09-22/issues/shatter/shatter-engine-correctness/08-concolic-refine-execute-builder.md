---
slug: concolic-refine-execute-builder
kind: new
title: "Concolic refine phase drops prepare_id/execution_profile and discards the paths it reaches; introduce one shared Execute-request builder"
priority: P2
type: bug
labels: [concolic, orchestrator, refactor, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Concolic refine phase drops prepare_id/execution_profile and discards the paths it reaches; introduce one shared Execute-request builder

## Problem

`refine_boundaries_async` builds its own `Command::Execute` requests with `prepare_id: None` and `execution_profile: None`. This drops TS execution adapters and forces re-preparation. It uses the results only to update boundary witnesses (true/false witnesses), so paths and lines reached during refinement never reach `covered_paths`, `raw_results` or the report.

The root cause is that each engine builds `Command::Execute` inline at eight or more sites, and each site re-decides `prepare_id`, `execution_profile`, `setup_context` and `capture`. The same pattern causes the open capture-flag bug str-qwua7.5, which this issue links but does not re-describe.

## Evidence

Line numbers were re-checked against `56c86168`:

- `shatter-core/src/orchestrator.rs:2321` `async fn refine_boundaries_async`. The Execute at `:2374-2384` sends `setup_context: setup_context.clone(), capture: false, prepare_id: None, execution_profile: None`.
- Refine results update only the tw/fw witnesses. They are not fed into observation or coverage accounting.
- The concolic `Classify` artifact from the audit (Go, nested x>0.5 / x<1) had `boundary_results` true_witness `[0.5000037571385455]` (20 executions) while `unique_paths=2`, and the report said 4/5 lines. The verifier noted that the witness is on branch 0 (x>0.5), and the artifact does not show whether it took the x<1 'low' side. The "forces a rebuild per execute on Go/Rust" consequence is not verified.
- Inline Execute construction sites in `orchestrator.rs` (by `capture:` literal): :1694, :2380, :2701, :2714, :3103, :3643, :3687, :3741. The random explorer has more (`explorer.rs` shrink sites at :1778/:1823/:1875 and the float-probe sites). str-qwua7.5 tracks the capture-flag part of this.

## Acceptance criteria

- [ ] A single helper builds every `Command::Execute` request from the engine config: `prepare_id`, `execution_profile`, `setup_context`, `capture`, plus mocks and plan. All Execute sites in both `orchestrator.rs` and `explorer.rs` use it. A grep for `Command::Execute {` / `ProtoCommand::Execute {` outside the helper and tests returns nothing, and the close note records that grep.
- [ ] Refine-phase executions go through normal observation and coverage accounting, so paths and lines reached only during refinement appear in `unique_paths`, `raw_results` and the report.
- [ ] Test: a TS execution-adapter target keeps its `execution_profile` (and `prepare_id`) on refine-phase Execute requests. Assert on the requests sent, for example through a recording frontend double.
- [ ] Test: a path reached only in the refine phase appears in the explore report and artifact. At close, show it failing on current `main` and passing after the fix.
- [ ] str-qwua7.5 is closed through this helper, or updated with a comment saying the helper is the fix vehicle. Do not duplicate its capture-flag description here.
- [ ] `task affected` (with `Gates selected` recorded) and `task e2e` pass.

## Suggested approach

Add an `ExecuteRequestBuilder`, or a function on the engine config, that owns the field decisions. Migrate the sites one by one (refine first), then route the refine results through the same aggregator call the main loop uses. `setup_context` should come from the pipeline-level helper in concolic-setup-teardown when that lands, but this issue does not depend on it: the builder can take the context it is given.

## Out of scope

- The capture-flag semantics themselves (str-qwua7.5).
- Session-lifecycle extraction (str-inct (b)).
- Broader shared-shrink extraction (str-qwua7.6 / str-qwua7.6.1).

## Priority

P2

## Type

bug

## Dependencies

- Blocked by: none.
- Related: str-qwua7.5 (open; core-16 is a duplicate of it, so link only), str-qwua7.6, str-inct, concolic-setup-teardown, float-probe-paths-uncounted (same "executions not counted" class of bug in the other engine).

## References

Audit 2026-09-22 findings core-06 (verified, P2) and core-16 (a duplicate of open str-qwua7.5, context only). Source draft: `drafts/shatter-code/16-concolic-refine-phase-execute-builder.md`.
