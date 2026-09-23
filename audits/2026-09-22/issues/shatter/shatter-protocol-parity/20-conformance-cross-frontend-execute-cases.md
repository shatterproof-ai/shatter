---
slug: conformance-cross-frontend-execute-cases
kind: new
title: "Conformance: every analyze/execute success case runs on one frontend, so the cross-frontend comparison never sees side_effects or conditions"
priority: P2
type: task
labels: [conformance, protocol, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [conformance-harness-correctness, conformance-known-drifts-matching]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Conformance: every analyze/execute success case runs on one frontend, so the cross-frontend comparison never sees side_effects or conditions

Split out of conformance-harness-correctness (Codex cross-check: that draft bundled four deliverables).

## Problem

The conformance harness cross-checks a case's responses across frontends only when more than one frontend runs it. Every analyze and execute success case in `conformance_cases.yaml` is restricted to one frontend, so the fields where cross-frontend drift actually lives (`side_effects`, `path_constraints`/conditions, outcome shapes) are never compared. The cross-check only ever compares error, handshake, setup, teardown, generate and shutdown responses.

## Evidence (re-verified 2026-09-23 at 56c86168; unchanged at 793f2b0b)

- Every analyze/execute success case in `conformance_cases.yaml` (roughly `:300-563`, e.g. `execute_outcome_shape_go`/`_ts`/`_rust`, `analyze_runtime_value_go`) lists a single frontend. Multi-frontend cases are only error, handshake, setup, teardown, generate and shutdown cases.
- The drift-patrol log contained 8 lines of `cross-check: SKIP only 1 frontend responded` (`conformance_harness.py:656`).
- Audit finding protocol-parity-02 (confirmed, P1 → P2; coverage part).

## Acceptance criteria

- [ ] At least one analyze success case and one execute success case are expressed once and run on every language frontend (TS, Go, Rust) against equivalent fixtures: the same function semantics in each language, with a branch whose condition and at least one side effect differ by path. The cases are not split into per-frontend copies, so the structural comparison runs.
- [ ] The execute case covers `side_effects` (at least one captured kind that the matrix marks captured for all three), `path_constraints`/conditions, and the outcome shape.
- [ ] Each case asserts the success status only; `status_oneof` must not include `error`.
- [ ] Prerequisites (analyze → instrument → prepare → execute, per frontend) are declared with the mechanism from conformance-harness-correctness.
- [ ] Every cross-frontend difference the new cases surface is either fixed or registered through the policy chosen in conformance-known-drifts-matching, with a matrix `allowed_divergences` id. No difference is silenced by narrowing the case back to one frontend.
- [ ] Proof at close: the run output shows `cross-check: structures match` (or a registered-divergence warning) for the new cases on all three frontends, with no `SKIP only 1 frontend responded` for them. Paste that section. Also show that the check is live: temporarily change one frontend's fixture so a side-effect kind differs, and paste the resulting DRIFT failure. Then revert.
- [ ] `task conformance` passes on the unmutated tree; paste the output of a run that executed (not checksum-cached).

## Out of scope

- Per-command success coverage for every implemented command (conformance-success-case-per-command).
- Schema validation of responses (protocol-schemas-reject-real-output).

## Dependencies

- Blocked by: conformance-harness-correctness (prerequisite declaration and replay), conformance-known-drifts-matching (a working way to register the drifts these cases will surface).
- Related: str-qe9pp (open; Rust prepare timeout, which may surface on the Rust leg).

Size: M. Priority: P2. Type: task. Labels: conformance, protocol, parity, audit. Parent: Epic: Audit 2026-09-22 findings.
