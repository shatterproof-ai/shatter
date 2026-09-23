---
slug: parity-dispatch-reconciliation
kind: new
title: "Parity gates never check dispatch: a command can be advertised but not dispatched and still pass every static gate"
priority: P2
type: task
labels: [parity, protocol, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Parity gates never check dispatch: a command can be advertised but not dispatched and still pass every static gate

## Problem

`scripts/validate-parity.py` compares only the capability lists that each frontend advertises in its handshake. `scripts/validate-protocol-registry.py` only *warns* when a command is missing. Neither script checks that each command is dispatched. During the audit, a scratch-copy mutation removed `prepare` dispatch from all three frontends and left the handshake advertising it. Both validators still exited 0 ("Parity check passed."). The golden handshake files compare only the advertised list. The only `prepare` conformance case runs only on Rust. No gate owns the end-to-end claim that a command marked implemented is actually dispatched.

## Evidence (re-verified 2026-09-23 at 56c86168)

- `scripts/validate-parity.py:294-386` has three detectors. `detect_typescript` reads `SUPPORTED_CAPABILITIES` from `handlers.ts` (:305-316). `detect_go` reads `CommandCapabilities` and `handleHandshake` (:321-353). `detect_rust` reads `handle_handshake` (:355-386). None of them parse the dispatch arms.
- `scripts/validate-protocol-registry.py:641-669` `validate()` skips empty source sets (`if not src_set: continue`, :647) and reports missing commands as `(may be unimplemented)` warnings (:664-666). The script exits 0: `python3 scripts/validate-protocol-registry.py` → `All checks passed (with informational warnings).`
- Dispatch sites that no gate reads: `shatter-ts/src/handlers.ts:556` (`case "prepare": {`), `shatter-go/protocol/handler.go:273` (`case "prepare":`), `shatter-rust/src/handler.rs:547` (`"prepare" => (self.handle_prepare(resp, req), false),`).
- `protocol/conformance/conformance_cases.yaml:167` `prepare_supported_rust` is the only prepare success case, with `frontends: [rust]`. The matrix (`protocol/parity-matrix.yaml:127`) marks prepare implemented for all three frontends.
- Audit finding protocol-parity-01. The verifier confirmed it from code without re-running the mutation, and downgraded it P1 → P2: this is a latent gate gap, and the E2E suites do exercise prepare at runtime.

## Acceptance criteria

- [ ] `validate-parity.py` extracts the dispatch arms for TS (`handlers.ts` switch), Go (`handler.go` switch) and Rust (`handler.rs` match). It hard-fails (non-zero exit) in both directions: (a) the matrix marks a command `implemented` for a frontend, or the handshake advertises it, but the command is not dispatched; (b) a command is dispatched but neither advertised nor listed in the matrix. The base-protocol commands `handshake` and `shutdown` are required for every frontend.
- [ ] An extractor that finds zero dispatch arms for a frontend is a hard error, never a silent pass.
- [ ] Every (frontend × command the matrix marks implemented) pair has at least one minimal runtime conformance case in `conformance_cases.yaml`, generated or hand-written. Hand-written cases need a test that fails when a pair has no case.
- [ ] Proof at close: a scripted mutation test (unit test in `scripts/test_validate_parity.py` or equivalent, operating on fixture copies) removes one dispatch arm per frontend while keeping the advertisement, and asserts the gate exits non-zero. Paste the failing-then-passing output into the close note.
- [ ] `task parity` and `task conformance` pass after being forced to execute (not checksum-cached); record the output.

## Suggested approach

Put the dispatch extractors in one shared helper module used by `validate-parity.py`. The TS/Go/Rust extractor work in validator-ts-extraction-empty can then reuse it rather than growing a second regex set. Prefer generating the per-command conformance cases from the matrix over hand-writing 3×N entries.

## Out of scope

- Fixing harness timeouts, known_drifts or the summary line (conformance-harness-correctness).
- Deriving the matrix, registry and golden copies from one source (capability-single-source).

## Dependencies

- Blocked by: none.
- Related: validator-ts-extraction-empty (shared TS dispatch extractor), conformance-harness-correctness (new cases run through the harness), str-qwua7.7 (closed; its dispatch extractor lives in the registry validator and only warns), str-2fjn (option a: one conformance case per command).

Size: M. Priority: P2. Type: task. Labels: parity, protocol, quality-gates, audit. Parent: Epic: Audit 2026-09-22 findings.
