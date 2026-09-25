# Rewrite frontend-parity skill workflow and extend frontend-issue-template parity checklist to Go/Rust builders

- Priority: P3
- Type: task
- Labels: agents,skills,parity
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (partially covered by str-qwua7.24)
- Source findings: protocol-parity-17, protocol-parity-18
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
Parity guidance loaded by agents is stale. The frontend-parity skill's
hand-copied capability table contradicts `protocol/parity-matrix.yaml`, and
its "When you're about to..." steps route agents to `known_drifts`, a
mechanism that cannot match anything (regex patterns tested as substrings in
`conformance_harness.py:675`). The frontend issue template's parity checklist
names only the TS builders.

## Current Code Facts
- `.claude/skills/frontend-parity/SKILL.md` last changed 2026-05-05.
  :37 Go thrown_error ✗ (matrix: `go: captured`); :59 "Rust's analyze handler
  is a stub"; :69 Rust timeout "stored, not yet applied"; :74-80 "document the
  drift in known_drifts"; no mention of `allowed_divergences`,
  `scripts/validate-parity.py` or the PARITY.md mirror rule.
- `protocol/frontend-issue-template.md:11-19` lists TS `buildSymExpr`/
  `buildSymExprWithFlow`, explorer/orchestrator and main.rs CLI wiring only.
  Missing: Go `protocol/analyzer.go` vs `instrument/symextract.go` (and loop
  snapshot builder in `protocol/loop_body_states.go`), Rust
  `analyzer.rs build_sym_expr` vs `instrument.rs constraint_for_expr`, and a
  "protocol-visible → matrix entry or divergence + task parity/conformance" item.

## Acceptance Criteria
- The skill's capability tables are removed or generated (per str-qwua7.24);
  the workflow steps point at `allowed_divergences` in parity-matrix.yaml plus
  `task parity` and `task conformance`; `known_drifts` advice removed unless
  the harness is fixed to use `re.search`.
- The template lists the Go and Rust builder pairs and the matrix/divergence step.

## Out of Scope
Fixing the conformance harness known_drifts matching (product finding).
