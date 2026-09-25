---
slug: divergence-tracking-issue-liveness
kind: new
title: "Parity divergences marked `tracked` point at closed or deferred issues; add a tracking-issue liveness check"
priority: P2
type: task
labels: [parity, protocol, drift, agents, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Parity divergences marked `tracked` point at closed or deferred issues; add a tracking-issue liveness check

## Problem

Entries in `protocol/parity-matrix.yaml` `allowed_divergences` with `status: tracked` are supposed to have a live owning issue. Three of them point at issues that are closed or deferred, so no one owns those divergences. Closing an implementation issue never prompts anyone to repoint the divergences that cite it. The validator checks only that `tracking_issue` is a non-empty string.

## Evidence (re-verified 2026-09-23 at 56c86168; tracker state via `bd show`)

- `go-symbolic-http-request-body` (`parity-matrix.yaml:1001`) is `tracking_issue: str-e41w`, and `bd show str-e41w` → CLOSED. Its `affected_frontends` are `[typescript, rust]`: the Go work is done and the gap is in TS/Rust, so the `go-` prefix is misleading. The ID is also referenced in `protocol/PARITY.md` and `shatter-go/CLAUDE.md`.
- `ite-symexpr-production-partial` (`:1096`, `affected_frontends: [rust]`) is `tracking_issue: str-1hlk.17`, which is CLOSED (it was the Go ite epic). No open issue covers Rust ite production.
- `ts-rust-execute-plan-not-implemented` (`:937`) is `tracking_issue: str-1hlk.16`, which is DEFERRED.
- `scripts/validate-parity.py:515-521` checks only that `tracking_issue` is a non-empty string or `none`.
- `scripts/drift-patrol.py:185-215` already loads tracker state: `bd list --all --json`, falling back to `.beads/issues.jsonl` when `bd` is absent (CI).
- Audit finding protocol-parity-12 (confirmed, P2).

## Acceptance criteria

- [ ] A drift-patrol check (it has tracker access; `validate-parity.py` stays tracker-free) FAILs when a `tracked` divergence's `tracking_issue` is closed or missing, and WARNs when it is deferred. Unit tests cover open, closed, missing and deferred using a fake tracker.
- [ ] When the only tracker source is the stale `.beads/issues.jsonl` export, the check reports SKIP with a reason instead of judging liveness from it. This is consistent with D4; see beads-jsonl-consumers-drop-bd-sync.
- [ ] For the two closed-issue entries (Rust ite production; TS/Rust symbolic request-body synthesis), successor issues are filed and the entries repointed, or the entries are reclassified `accepted` with a reason. str-1hlk.16 is either un-deferred or the entry gets a WARN-acknowledged reason.
- [ ] The misnamed ID is renamed (e.g. `native-request-body-synthesis-go-only`), and every reference is updated (matrix, PARITY.md, shatter-go/CLAUDE.md; find them with `git grep go-symbolic-http-request-body -- . ':!audits' ':!.beads'`).
- [ ] Proof at close: the new check run against the current matrix (before repointing) reports the two closed issues as FAIL (paste the output). `task parity` and `task drift-patrol` pass afterwards.

## Suggested approach

Add a `divergence-tracking-liveness` check to drift-patrol that reuses its existing Tracker loader. Keep `validate-parity.py` offline and deterministic.

## Out of scope

Implementing the divergent features (Rust ite, TS/Rust request-body synthesis, TS/Rust InvocationPlan).

## Dependencies

- Blocked by: none.
- Related: str-1hlk.12 (closed; requires a tracking issue but not a live one), str-qwua7.34 (divergence-ID resolution), beads-jsonl-consumers-drop-bd-sync (D4; decides what CI reads instead of the JSONL), drift-patrol-workflow-go-mod.

Size: S. Priority: P2. Type: task. Labels: parity, protocol, drift, agents, audit. Parent: Epic: Audit 2026-09-22 findings.
