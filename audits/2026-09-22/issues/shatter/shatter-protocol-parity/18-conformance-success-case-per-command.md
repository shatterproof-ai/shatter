---
slug: conformance-success-case-per-command
kind: new
title: "Conformance: no executed success case exists for most (frontend, implemented command) pairs; existing cases accept error responses or are skipped by capability"
priority: P2
type: task
labels: [conformance, parity, protocol, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [conformance-harness-correctness]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Conformance: no executed success case exists for most (frontend, implemented command) pairs; existing cases accept error responses or are skipped by capability

Split out of parity-dispatch-reconciliation (the runtime half of that gap). The static dispatch check stays there.

## Problem

`protocol/parity-matrix.yaml` marks each command `implemented` per frontend, but no runtime gate proves that each such pair works. Three loopholes let a conformance case "cover" a pair without showing a successful implementation:

1. Cases such as `setup_session` and `generate_value` declare `status_oneof: [setup, error]` / `[generate, error]`, so an error response passes.
2. The harness skips any command a frontend does not advertise in its handshake (`conformance_harness.py:594-599`), so dropping a command from the handshake turns its case into a SKIP rather than a failure.
3. Many pairs have no case at all. The only `prepare` success case is `prepare_supported_rust` (`conformance_cases.yaml:167`, `frontends: [rust]`), although the matrix marks prepare implemented for all three frontends.

Some commands also need prior state. TS `prepare` needs a prior `instrument` of the same function, and Go `get_invocation_plan` needs a prior `analyze` (`shatter-go/protocol/handler.go:1960`).

## Evidence (re-verified 2026-09-23 at 56c86168; unchanged at 793f2b0b)

- `conformance_cases.yaml:89-113` `setup_session`: `status_oneof: [setup, error]` ("Real frontends may return error for nonexistent file paths"). `:137-160` `generate_value`: `status_oneof: [generate, error]`.
- `conformance_harness.py:594-599`: `if fp.capabilities and cmd not in ("handshake", "shutdown") and not ignore_capability_check: if cmd not in fp.capabilities: … _skip('(not in capabilities)')`.
- `conformance_cases.yaml:167` `prepare_supported_rust`; `parity-matrix.yaml:127` prepare implemented for ts, go and rust.
- Audit finding protocol-parity-01 (confirmed, P1 → P2; runtime part).

## Acceptance criteria

- [ ] For every (frontend, command) pair that `parity-matrix.yaml` marks `implemented`, at least one case: (a) runs on that frontend; (b) sends a valid request against a real fixture, after its declared prerequisites (conformance-harness-correctness mechanism); (c) asserts the command's success status only, with no `error` in `status_oneof`; (d) asserts at least one command-specific field of the success response (for example `setup_context` for setup, `value` for generate, the prepared handle for prepare).
- [ ] A case for a matrix-implemented pair may not be skipped because the command is missing from the frontend's handshake. For such pairs the harness treats "not in capabilities" as a failure; `ignore_capability_check` is not a way around this.
- [ ] A test (in `scripts/` or the harness's own test file) reads the matrix and `conformance_cases.yaml` and fails when an implemented pair has no qualifying success case, or when its only case accepts `error`. Existing error-tolerant cases may remain as separate error-path cases.
- [ ] Proof at close: (1) run the coverage test against the pre-change `conformance_cases.yaml` and paste the list of uncovered pairs it reports; (2) remove the TS `prepare` dispatch arm in a scratch copy and paste the resulting conformance FAIL (not SKIP) for `ts / prepare`; (3) remove `prepare` from the TS handshake list only, and paste the FAIL it produces.
- [ ] `task conformance` passes on the unmutated tree; paste the output of a run that executed (not checksum-cached).

## Suggested approach

Generate the per-pair cases from the matrix plus a small per-command request template, rather than hand-writing 3×N entries. If str-2fjn picks its option (a), a conformance case per command generated from the registry field list, share one generator: this issue owns the per-frontend success assertion, and str-2fjn owns populating every declared field.

## Out of scope

- The static dispatch-vs-advertisement check (parity-dispatch-reconciliation).
- Cross-frontend structural comparison of analyze/execute (conformance-cross-frontend-execute-cases).

## Dependencies

- Blocked by: conformance-harness-correctness (prerequisite declaration/replay; without it, prepare and get_invocation_plan cases are order-dependent).
- Related: parity-dispatch-reconciliation, str-2fjn (open; option (a) is a per-command generated case for field placement), str-qe9pp (open; Rust prepare timeout).

Size: M. Priority: P2. Type: task. Labels: conformance, parity, protocol, quality-gates, audit. Parent: Epic: Audit 2026-09-22 findings.
