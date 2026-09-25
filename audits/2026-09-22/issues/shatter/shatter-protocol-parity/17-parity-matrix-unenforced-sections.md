---
slug: parity-matrix-unenforced-sections
kind: new
title: "Four parity-matrix sections (shared_wire_types, side_effect_capabilities, feature_capabilities, adapter_capabilities) are read by no gate but are presented as authoritative"
priority: P2
type: task
labels: [parity, protocol, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Four parity-matrix sections (shared_wire_types, side_effect_capabilities, feature_capabilities, adapter_capabilities) are read by no gate but are presented as authoritative

Split out of capability-single-source (Codex cross-check: that draft was still L-sized after the codegen split).

## Problem

`protocol/parity-matrix.yaml` has four sections that no script reads: `shared_wire_types`, `side_effect_capabilities`, `feature_capabilities` and `adapter_capabilities`. Agents still treat them as enforced. The frontend-parity skill calls the matrix "The authoritative matrix" and points at `side_effect_capabilities` specifically. A frontend can stop capturing a side effect, or drop a feature, and the matrix will go on claiming it with every gate green.

## Evidence (re-verified 2026-09-23 at 56c86168; unchanged at 793f2b0b)

- Section locations: `parity-matrix.yaml:18` (`shared_wire_types`), `:482` (`side_effect_capabilities`), `:589` (`feature_capabilities`), `:881` (`adapter_capabilities`).
- `git grep -lE 'side_effect_capabilities|feature_capabilities|adapter_capabilities|shared_wire_types' -- . ':!audits' ':!.beads'` returns only `parity-matrix.yaml`, `conformance_cases.yaml` (comments), the frontend-parity skill, three crate `CLAUDE.md` files, and prose comments in `shatter-cli/src/commands/explore.rs` (e.g. `:1849`, `:1907`, `:2116`). No file under `scripts/` or `protocol/conformance/*.py` reads them.
- `.claude/skills/frontend-parity/SKILL.md:22`: "`protocol/parity-matrix.yaml` | **The authoritative matrix.** `side_effect_capabilities` enumerates which frontend captures which side effect kinds."
- Audit finding protocol-parity-08 (confirmed, P2; unvalidated-sections part).

## Acceptance criteria

- [ ] Each of the four sections is classified in the matrix itself as `enforced: true` or `enforced: false`, and `validate-parity.py` fails when a top-level section lacks the marker.
- [ ] Every `enforced: true` section has a detector that `task parity` or `task conformance` runs. Examples: a source check for each side-effect emitter a frontend is marked `captured` for, or a conformance execute case asserting that the side-effect kind is present in the response. At minimum, `side_effect_capabilities` must be enforced, because the skill directs agents to it.
- [ ] Every `enforced: false` section carries a header comment saying it is documentation only, and `validate-parity.py` prints the list of unenforced sections on every run.
- [ ] Proof at close, for each enforced section: flip one frontend's entry in that section only (for example Go `thrown_error: captured` → `not_captured`, with no source change). Paste the **pre-change** gate exit 0 on that mutation, then the post-change non-zero exit naming the section and frontend. Revert.
- [ ] Cache wiring: `protocol/parity-matrix.yaml`, `scripts/validate-parity.py` and any new detector input are in the `sources:` of the task that runs the detector (task-sources-cover-real-inputs adds the first two to `parity.sources`; add them here if it has not landed). Proof: `touch protocol/parity-matrix.yaml && task parity` (ordinary invocation) executes the validator rather than printing `is up to date`; paste the output.

## Out of scope

- The registry `frontends:` block (capability-single-source).
- Skill and crate `CLAUDE.md` tables (str-qwua7.24) and the skill's "authoritative" wording (parity-guidance-skill-and-template).

## Dependencies

- Blocked by: none.
- Related: capability-single-source, str-qwua7.24, conformance-cross-frontend-execute-cases (its execute case can double as the side-effect detector), task-sources-cover-real-inputs (shatter-gates-integrity bucket).

Size: M. Priority: P2. Type: task. Labels: parity, protocol, quality-gates, audit. Parent: Epic: Audit 2026-09-22 findings.
