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

## Evidence (re-verified 2026-09-23 at 56c86168; unchanged at 793f2b0b)

- `scripts/validate-parity.py:294-386` has three detectors. `detect_typescript` reads `SUPPORTED_CAPABILITIES` from `handlers.ts` (:305-316). `detect_go` reads `CommandCapabilities` and `handleHandshake` (:321-353). `detect_rust` reads `handle_handshake` (:355-386). None of them parse the dispatch arms.
- `scripts/validate-protocol-registry.py:641-669` `validate()` skips empty source sets (`if not src_set: continue`, :647) and reports missing commands as `(may be unimplemented)` warnings (:664-666). The script exits 0: `python3 scripts/validate-protocol-registry.py` → `All checks passed (with informational warnings).`
- Dispatch sites that no gate reads: `shatter-ts/src/handlers.ts:556` (`case "prepare": {`), `shatter-go/protocol/handler.go:273` (`case "prepare":`), `shatter-rust/src/handler.rs:547` (`"prepare" => (self.handle_prepare(resp, req), false),`).
- `protocol/conformance/conformance_cases.yaml:167` `prepare_supported_rust` is the only prepare success case, with `frontends: [rust]`. The matrix (`protocol/parity-matrix.yaml:127`) marks prepare implemented for all three frontends.
- Audit finding protocol-parity-01. The verifier confirmed it from code without re-running the mutation, and downgraded it P1 → P2: this is a latent gate gap, and the E2E suites do exercise prepare at runtime.

## Acceptance criteria

- [ ] `validate-parity.py` extracts the dispatch arms for TS (`handlers.ts` switch), Go (`handler.go` switch) and Rust (`handler.rs` match). It hard-fails (non-zero exit) in both directions: (a) the matrix marks a command `implemented` for a frontend, or the handshake advertises it, but the command is not dispatched; (b) a command is dispatched but neither advertised nor listed in the matrix. The base-protocol commands `handshake` and `shutdown` are required for every frontend.
- [ ] An extractor that finds zero dispatch arms for a frontend is a hard error, never a silent pass. A unit test feeds each extractor an empty/renamed source file and asserts the non-zero exit.
- [ ] Mutation tests in `scripts/test_validate_parity.py` (operating on fixture copies, not the live tree) cover, per frontend: (1) dispatch arm removed while the handshake still advertises it; (2) a dispatch arm added for a command that is neither advertised nor in the matrix. Each asserts a non-zero exit **and** names the frontend and command in the error.
- [ ] Proof at close that the new check is what catches the mutation: run the **pre-change** `validate-parity.py` against mutation (1) and paste its exit 0 ("Parity check passed."), then run the post-change script against the same mutation and paste its non-zero exit. A canary that the old gate already rejects does not count.
- [ ] Cache wiring: every file the new extractors read (`shatter-ts/src/handlers.ts`, `shatter-go/protocol/handler.go`, `shatter-rust/src/handler.rs`) and `scripts/validate-parity.py` itself are covered by `parity.sources` in `Taskfile.yml` (validate-parity.py is currently missing; task-sources-cover-real-inputs adds it. If that issue has not landed, add it here). Proof: after the change, `touch scripts/validate-parity.py && task parity` (ordinary invocation, no `--force`) executes the validator rather than printing `is up to date`; paste the output.
- [ ] `task parity` passes on the unmutated tree; paste the output of a run that executed (not checksum-cached).

## Suggested approach

Put the dispatch extractors in one shared helper module used by `validate-parity.py`. The TS/Go/Rust extractor work in validator-ts-extraction-empty can then reuse it rather than growing a second regex set.

## Out of scope

- Runtime conformance coverage (a successful conformance case per implemented frontend × command pair). Split out to conformance-success-case-per-command, because a static dispatch check and runtime cases are separate deliverables.
- Fixing harness timeouts, known_drifts or the summary line (conformance-harness-correctness, conformance-known-drifts-matching).
- Deriving the matrix, registry and golden copies from one source (capability-single-source).

## Dependencies

- Blocked by: none.
- Related: validator-ts-extraction-empty (shared TS dispatch extractor), conformance-success-case-per-command (runtime half of this gap), task-sources-cover-real-inputs (shatter-gates-integrity bucket; adds `validate-parity.py` and the matrix to `parity.sources`), str-qwua7.7 (closed; its dispatch extractor lives in the registry validator and only warns), str-2fjn (option a: one conformance case per command).

Size: S-M. Priority: P2. Type: task. Labels: parity, protocol, quality-gates, audit. Parent: Epic: Audit 2026-09-22 findings.
