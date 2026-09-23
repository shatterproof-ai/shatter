---
slug: governance-md-omits-matrix
kind: new
title: "protocol/GOVERNANCE.md, the required protocol-change checklist, omits the parity matrix, codegen and validate-parity, and names two authorities"
priority: P2
type: task
labels: [protocol, parity, docs, governance, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# protocol/GOVERNANCE.md, the required protocol-change checklist, omits the parity matrix, codegen and validate-parity, and names two authorities

## Problem

Root `CLAUDE.md` and the frontend-parity skill send agents to `protocol/GOVERNANCE.md` as the checklist to follow for any protocol change. The file was last changed on 2026-05-05 (84a44654). Since then, the protocol-change machinery has grown to include codegen (`scripts/protocol-codegen.py`), `scripts/validate-parity.py`, the `parity-matrix.yaml` capability and `allowed_divergences` sections, and the generated TS/Go enum files. GOVERNANCE mentions none of them.

Following GOVERNANCE step by step therefore leaves the matrix, the generated enums and the divergence metadata out of date. The gaps show up later as `task parity` failures or as silent drift where no gate looks.

The file also names two sources of truth, the registry and core `protocol.rs`. It tells agents to record drift in `known_drifts`, which cannot match anything (see conformance-harness-correctness). And it describes a "source-name parity layer" for every frontend that is silently empty for TS (see validator-ts-extraction-empty).

## Evidence (re-verified 2026-09-23 at 56c86168)

- `/usr/bin/grep -cE 'parity-matrix|validate-parity|protocol-codegen|generated|PARITY.md|allowed_divergences' protocol/GOVERNANCE.md` → `0`.
- Two authorities: `GOVERNANCE.md:7` says "`protocol/registry.yaml` is the **single source of truth**". `:55-57` ("4. Implement in shatter-core (authoritative)") says core `protocol.rs` "is the authoritative implementation — frontends must match it".
- Step 1 (`:31`) never says to regenerate bindings (`python3 scripts/protocol-codegen.py --write`).
- The step 5 table (`:61-71`) points TS at `shatter-ts/src/protocol.ts`, but the vocabulary now lives in `shatter-ts/src/generated/protocol-enums.ts` and dispatch lives in `handlers.ts`. It points Rust at `shatter-rust/src/protocol.rs` only, but dispatch is in `shatter-rust/src/handler.rs`.
- `:78`, `:185`, `:209` and `:211-215` tell agents to record accepted differences in `known_drifts`. `allowed_divergences` in the matrix is never mentioned.
- `:96` says "Run **all five** checks". The list covers registry, schema, conformance, golden and per-language checks, and omits `protocol-codegen.py --check` and `validate-parity.py`. Both run in `task parity` (`Taskfile.yml:265-274`).
- `:113-114` "Source-name parity layer. Cross-checks command, response status, and error code names against source files in core and every frontend." TS extraction returns empty sets and is skipped (`scripts/validate-protocol-registry.py:545-553`, `:647`).
- CI Integration (`:173-181`) names `task schemas`, `task conformance` and `task golden-test`. It does not mention `task parity`, or the `task check` gate that CI actually runs.
- Audit finding protocol-parity-07 (confirmed, P2). Report section 15.1 lists it as a new issue with no prior draft.

## Acceptance criteria

- [ ] GOVERNANCE.md names exactly one authority for vocabulary and field model: the registry. It states that core serde must match the registry and names the test that enforces this. No other sentence calls a different file authoritative.
- [ ] The required steps form one ordered checklist, and every step a real protocol change needs is on it:
  1. registry
  2. `python3 scripts/protocol-codegen.py --write`
  3. core serde types
  4. each frontend, with its files listed per frontend, including dispatch files
  5. schemas and fixtures
  6. `parity-matrix.yaml` status, plus an `allowed_divergences` entry for any intended gap
  7. PARITY.md / PROTOCOL.md, per whatever protocol-parity-md-stale and protocol-md-execute-fields decide
  8. `task parity`, `task conformance` and `task schemas`
- [ ] The Validation Checks section lists every check that `task parity`, `task conformance` and `task schemas` actually run. A test (e.g. in `scripts/`) parses the command lists of those Taskfile tasks and fails if GOVERNANCE omits one. That is the doc-to-Taskfile consistency check that was missing.
- [ ] known_drifts guidance matches the outcome of conformance-harness-correctness (kept with `divergence_id`, or deleted). Either way, `allowed_divergences` is named as the registry of intended divergences.
- [ ] The source-name parity layer description matches what validator-ts-extraction-empty decides. If that issue is still open, describe the layer as it actually behaves and link the issue.
- [ ] Proof at close: the new consistency test fails against the pre-rewrite GOVERNANCE.md (paste the output) and passes after the rewrite.

## Suggested approach

Rewrite it as a short numbered checklist, and move the explanation into linked sections. Keep the per-frontend file table, but generate or `--check` it if capability-single-source adds a generator.

## Out of scope

Changing the validators themselves. Their fixes live in parity-dispatch-reconciliation, validator-ts-extraction-empty and conformance-harness-correctness. This issue documents what they do.

## Dependencies

- Blocked by: none. It can land first and describe current behavior. If conformance-harness-correctness or validator-ts-extraction-empty land first, their close notes decide the known_drifts and source-name wording.
- Related: str-2fjn and str-qwua7.7 (each asked for a one-sentence GOVERNANCE update), str-fpgb.9 (closed; created the original doc), conformance-harness-correctness, validator-ts-extraction-empty, protocol-schemas-reject-real-output, parity-guidance-skill-and-template.

Size: S. Priority: P2. Type: task. Labels: protocol, parity, docs, governance, audit. Parent: Epic: Audit 2026-09-22 findings.
