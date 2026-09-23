# validate-parity: fail when a `tracked` divergence points at a closed or missing issue

- Priority: P2
- Type: task
- Labels: parity,agents,drift
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (related str-1hlk.12, str-qwua7.34)
- Source findings: protocol-parity-12
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
parity-matrix divergences marked `tracked` point at issues that are closed or
deferred, so the divergence has no live owner. Closing an implementation issue
never prompts anyone to repoint divergences that cite it.

## Current Code Facts
- `protocol/parity-matrix.yaml`:
  `go-symbolic-http-request-body` → str-e41w (closed 2026-07-05); its
  `affected_frontends` are [typescript, rust], so the `go-` name is wrong.
  `ite-symexpr-production-partial` → str-1hlk.17 (closed 2026-05-12); no open
  issue for Rust ite.
  `ts-rust-execute-plan-not-implemented` → str-1hlk.16 (deferred).
- `scripts/validate-parity.py:515-521` checks only that `tracking_issue` is a
  non-empty string or "none".
- `scripts/drift-patrol.py` already reads tracker state.

## Acceptance Criteria
- A drift-patrol check (tracker access) FAILs when a `tracked` divergence's
  issue is closed or missing (WARN for deferred); unit-tested.
- Successor issues filed for Rust ite production and TS/Rust request-body
  synthesis, or those entries reclassified as `accepted` with a reason.
- The misnamed ID renamed (and references updated in PARITY.md/CLAUDE.md).
- `task parity` passes.

## Out of Scope
Implementing the divergent features.
