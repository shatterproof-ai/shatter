---
slug: capability-single-source
kind: new
title: "Protocol capability facts are hand-copied in six places, and four parity-matrix sections are checked by no script"
priority: P2
type: task
labels: [parity, protocol, architecture, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Protocol capability facts are hand-copied in six places, and four parity-matrix sections are checked by no script

## Problem

Which commands and capabilities each frontend supports is maintained by hand in six places:

1. the frontend handshake arrays
2. the `frontends:` block of `protocol/registry.yaml`
3. `protocol/parity-matrix.yaml`
4. the golden handshake files
5. `protocol/PARITY.md`
6. the crate `CLAUDE.md` files and the frontend-parity skill

Only pairwise checks exist, so a fact can be correct in one copy and wrong in another (see protocol-parity-md-stale for live examples). Four matrix sections, `shared_wire_types`, `side_effect_capabilities`, `feature_capabilities` and `adapter_capabilities`, are read by no script. Agents still treat them as enforced; the frontend-parity skill calls the matrix "authoritative".

## Evidence (re-verified 2026-09-23 at 56c86168)

- The copies: TS `SUPPORTED_CAPABILITIES` in `shatter-ts/src/handlers.ts`; Go `CommandCapabilities` and `handleHandshake` in `shatter-go/protocol/handler.go`; Rust `handle_handshake` in `shatter-rust/src/handler.rs`; `protocol/registry.yaml:407` (`frontends:`); `protocol/parity-matrix.yaml:102` (`commands:`) and `:203` (`complex_type_capabilities:`); `protocol/conformance/golden/handshake/{typescript,go,rust,noop}.json`; the `protocol/PARITY.md` tables; `shatter-{ts,go,rust}/CLAUDE.md`; `.claude/skills/frontend-parity/SKILL.md`.
- The unvalidated sections are at `parity-matrix.yaml:18` (`shared_wire_types`), `:482` (`side_effect_capabilities`), `:589` (`feature_capabilities`) and `:881` (`adapter_capabilities`). `git grep -lE 'side_effect_capabilities|feature_capabilities|adapter_capabilities|shared_wire_types' -- . ':!audits' ':!.beads'` returns only `parity-matrix.yaml`, `conformance_cases.yaml` (comments), the skill, three crate `CLAUDE.md` files and a prose mention in `shatter-cli/src/commands/explore.rs`. No file under `scripts/` reads them.
- Audit finding protocol-parity-08 (confirmed, P2).

## Acceptance criteria

- [ ] `protocol/parity-matrix.yaml` is the single source of per-frontend capability status. The registry `frontends:` block and the golden handshake expectations are generated from it, or checked against it by a generator `--check` that runs in `task parity`. Handshake arrays in frontend source stay hand-written but are checked against the matrix. The existing detectors already do that; keep them.
- [ ] Each of the four unvalidated sections either gets a detector that `task parity` runs (for example a source grep for each side-effect emitter, or a conformance execute case that asserts presence), or gets a header comment and a matrix-level `enforced: false` marker that `validate-parity.py` reads and prints, so readers can tell documentation-only sections from enforced ones.
- [ ] Proof at close: a canary edit (flip one Rust complex-type capability in the matrix only) makes `task parity` fail when forced to execute. Paste the output. Then revert.
- [ ] The frontend-parity skill and crate `CLAUDE.md` tables are generated or pointer-only (coordinate with str-qwua7.24; do not duplicate its generator).

## Suggested approach

Extend the str-qwua7.24 table generator rather than writing a second one. Its `--check` mode should also cover the registry `frontends:` block and the golden handshake expectations. protocol-parity-md-stale can reuse the same generator for PARITY.md.

## Out of scope

- Generating the 13 registry enums for TS, Go and Rust (split out to protocol-codegen-all-registry-enums, because this issue is already L-sized).
- Dispatch-vs-advertisement reconciliation (parity-dispatch-reconciliation).

## Dependencies

- Blocked by: none.
- Related: str-qwua7.24 (generated doc tables), str-qwua7.21.3, str-2fjn, protocol-parity-md-stale, protocol-codegen-all-registry-enums, protocol-md-execute-fields.

Size: L. Priority: P2. Type: task. Labels: parity, protocol, architecture, audit. Parent: Epic: Audit 2026-09-22 findings.
