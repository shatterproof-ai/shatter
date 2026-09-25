---
slug: parity-guidance-skill-and-template
kind: new
title: "frontend-parity skill workflow steps point at dead known_drifts and never mention allowed_divergences or validate-parity; the frontend-issue-template parity checklist omits the Go and Rust builders"
priority: P3
type: task
labels: [agents, skills, parity, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# frontend-parity skill workflow steps point at dead known_drifts and never mention allowed_divergences or validate-parity; the frontend-issue-template parity checklist omits the Go and Rust builders

## Problem

The parity guidance that agents load is stale. The `frontend-parity` skill's "When you're about to..." steps send agents to `known_drifts`, a mechanism that cannot currently match anything (see conformance-known-drifts-matching), and they never mention `allowed_divergences` or `validate-parity.py`. (The skill's hand-copied capability tables and its false capability prose are also stale, but those are owned by the open str-qwua7.24, which generates the tables from the matrix. This issue does not touch them.) The parity checklist in `protocol/frontend-issue-template.md` names only the TS SymExpr builders and the core explorer/orchestrator pair. It leaves out the Go and Rust dual-builder pairs, which is where str-qwua7.35 and str-qwua7.36 found drift.

## Evidence (re-verified 2026-09-23 at 56c86168; unchanged at 793f2b0b)

- `.claude/skills/frontend-parity/SKILL.md` was last changed on 2026-05-05 (56ac9c81).
  - Context only (owned by str-qwua7.24, not this issue):
    - `:37` shows Go `thrown_error` as ✗; the matrix has `go: captured`. The audit also found Go `file_write`, `network_request` and `environment_read` marked ✗ in the same table while the matrix says captured.
    - `:59` says "TS is the only frontend that produces `ite` … Rust's analyze handler is a stub". Go produces ite (str-1hlk.17.3), and Rust analyze is implemented.
    - `:69` says the Rust timeout is "stored, not yet applied — execute unimpl".
  - `:23` and `:75` tell agents to "document the drift in `known_drifts`".
  - The file has no mention of `validate-parity.py` or the PARITY.md mirror rule. `allowed_divergences` appears only as a table description (`:22`), never as a workflow step.
- `protocol/frontend-issue-template.md` "Parity Impact" (around lines 11-19) lists TS `buildSymExpr`/`buildSymExprWithFlow`, `explorer.rs`/`orchestrator.rs` and `main.rs` CLI wiring only. It is missing:
  - Go `shatter-go/protocol/analyzer.go` vs `shatter-go/instrument/symextract.go`, plus the loop-snapshot builder in `shatter-go/protocol/loop_body_states.go`
  - Rust `shatter-rust/src/analyzer.rs` `build_sym_expr` (:1954) vs `shatter-rust/src/instrument.rs` `constraint_for_expr` (:519)
  - a "protocol-visible? → matrix entry or `allowed_divergences` entry, then `task parity` + `task conformance`" item
- Audit findings protocol-parity-17 (confirmed, P3) and protocol-parity-18 (confirmed, P3).

## Acceptance criteria

- [ ] The skill's workflow steps (the "When you're about to..." list, `:73` onward, and the file-table row at `:23`) point at `allowed_divergences` in `parity-matrix.yaml` as the place to register an intended gap, and name `task parity` (`validate-parity.py`) and `task conformance` as the checks to run.
- [ ] The `known_drifts` advice at `:23` and `:75` is removed, or reworded to the form conformance-known-drifts-matching chose (for example "known_drifts entries must carry a `divergence_id`"). If that issue is still open, the wording is "do not add known_drifts entries; register the gap in `allowed_divergences`".
- [ ] The file-table row at `:22` no longer calls the whole matrix "authoritative" without qualification. It says that only gate-enforced sections are contractual, and points at the `enforced:` markers added by parity-matrix-unenforced-sections (or, if that is still open, names the four unenforced sections).
- [ ] The issue template's "Parity Impact" checklist lists the Go and Rust builder pairs above and a "protocol-visible? → matrix entry or `allowed_divergences` entry, then `task parity` + `task conformance`" item.
- [ ] The skill's capability tables (`:37-41` and nearby) and the capability prose at `:59` and `:69` are left to str-qwua7.24. This issue does not edit them, so the two changes do not conflict. If str-qwua7.24 has already landed, confirm in the close note that its generated regions are untouched.
- [ ] Proof at close: paste `/usr/bin/grep -n "known_drifts\|allowed_divergences\|validate-parity\|task parity" .claude/skills/frontend-parity/SKILL.md` before and after. After the change, every `known_drifts` hit matches the chosen policy, and `allowed_divergences` and `task parity` appear in the workflow steps. Also paste the template diff showing the new checklist items.

## Out of scope

Fixing the conformance harness known_drifts matching (conformance-known-drifts-matching). The skill and crate `CLAUDE.md` capability tables and capability prose, and their generator (str-qwua7.24).

## Dependencies

- Blocked by: none. If conformance-known-drifts-matching is still open, word the known_drifts guidance as "do not use; register in allowed_divergences".
- Related: str-qwua7.24 (open; owns the skill's capability tables and capability prose), conformance-known-drifts-matching, governance-md-omits-matrix, protocol-parity-md-stale, parity-matrix-unenforced-sections.

Size: S. Priority: P3. Type: task. Labels: agents, skills, parity, audit. Parent: Epic: Audit 2026-09-22 findings.
