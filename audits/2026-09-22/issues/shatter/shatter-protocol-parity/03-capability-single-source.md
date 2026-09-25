---
slug: capability-single-source
kind: new
title: "The registry `frontends:` capability lists duplicate the parity matrix and are read by no gate"
priority: P2
type: task
labels: [parity, protocol, architecture, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# The registry `frontends:` capability lists duplicate the parity matrix and are read by no gate

## Problem

Which commands and complex types each frontend supports is maintained by hand in six places:

1. the frontend handshake arrays
2. the `frontends:` block of `protocol/registry.yaml`
3. `protocol/parity-matrix.yaml`
4. the golden handshake files
5. `protocol/PARITY.md`
6. the crate `CLAUDE.md` files and the frontend-parity skill

Only pairwise checks exist. `validate-parity.py` compares the matrix with the handshake arrays, and `run_golden_tests.py` compares the golden handshake files with live handshakes, so copies 1, 3 and 4 are tied together transitively. No script reads the registry's `frontends.<fe>.command_capabilities` or `complex_type_capabilities`, so copy 2 can drift freely.

This issue owns copy 2: make the matrix the single source for the registry `frontends:` capability lists (or delete them). The other copies have their own owners:

- copy 5: protocol-parity-md-stale
- copy 6: str-qwua7.24 (open; generates the crate `CLAUDE.md` and frontend-parity skill tables from the matrix)
- the four matrix sections no script reads: parity-matrix-unenforced-sections (split out of this issue)
- generating all registry enums: protocol-codegen-all-registry-enums (split out earlier)

## Evidence (re-verified 2026-09-23 at 56c86168; unchanged at 793f2b0b)

- The copies: TS `SUPPORTED_CAPABILITIES` in `shatter-ts/src/handlers.ts`; Go `CommandCapabilities` and `handleHandshake` in `shatter-go/protocol/handler.go`; Rust `handle_handshake` in `shatter-rust/src/handler.rs`; `protocol/registry.yaml:407` (`frontends:`, with `command_capabilities` and `complex_type_capabilities` per frontend); `protocol/parity-matrix.yaml:102` (`commands:`) and `:203` (`complex_type_capabilities:`); `protocol/conformance/golden/handshake/{typescript,go,rust,noop}.json`.
- `/usr/bin/grep -n "command_capabilities\|complex_type_capabilities" scripts/*.py protocol/conformance/*.py` finds only `validate-parity.py`, which reads the matrix's `complex_type_capabilities` (`:233`, `:725-803`), and a test fixture. `validate-parity.py:649` and `:716` read `frontends` under matrix commands, not the registry block. Nothing reads `registry.yaml` `frontends.*.command_capabilities`.
- Audit finding protocol-parity-08 (confirmed, P2).

## Acceptance criteria

- [ ] `protocol/parity-matrix.yaml` is the single source of per-frontend command and complex-type status. The registry `frontends:` capability lists are either removed (with every reader updated) or generated from the matrix. A generator `--check` run in `task parity` fails when they differ.
- [ ] The handshake arrays in frontend source stay hand-written. The existing `validate-parity.py` detectors already check them against the matrix; keep those detectors.
- [ ] Proof at close that the new check is load-bearing. Mutation: remove `prepare` from `registry.yaml` `frontends.typescript.command_capabilities` only (or, if the lists are deleted, re-add a stale list and show the check rejects its presence). Paste the **pre-change** `task parity` exit 0 (run forced, so it executes), then the post-change non-zero exit naming the mismatch. A canary the old gate already rejects proves nothing (for example a matrix-only complex-type flip, which the existing handshake comparison already fails).
- [ ] Cache wiring: every input the new check reads (`protocol/parity-matrix.yaml`, the generator script) is in `parity.sources`. The matrix is currently missing; task-sources-cover-real-inputs adds it, so add it here if that issue has not landed. Proof: `touch protocol/parity-matrix.yaml && task parity` (ordinary invocation) executes the check rather than printing `is up to date`; paste the output.
- [ ] If the generator is shared with str-qwua7.24's table generator, extend that one rather than writing a second. This issue does not change the crate `CLAUDE.md` or skill tables.

## Out of scope

- Crate `CLAUDE.md` and frontend-parity skill capability tables (str-qwua7.24).
- PARITY.md (protocol-parity-md-stale).
- The four unvalidated matrix sections (parity-matrix-unenforced-sections).
- Generating all 13 registry enums (protocol-codegen-all-registry-enums).
- Dispatch-vs-advertisement reconciliation (parity-dispatch-reconciliation).

## Dependencies

- Blocked by: none.
- Related: str-qwua7.24 (open; table generator, reuse it), str-qwua7.21.3, str-2fjn, protocol-parity-md-stale, parity-matrix-unenforced-sections, protocol-codegen-all-registry-enums, protocol-md-execute-fields, task-sources-cover-real-inputs (shatter-gates-integrity bucket).

Size: S. Priority: P2. Type: task. Labels: parity, protocol, architecture, audit. Parent: Epic: Audit 2026-09-22 findings.
