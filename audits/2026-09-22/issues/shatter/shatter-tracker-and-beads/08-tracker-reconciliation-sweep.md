---
slug: tracker-reconciliation-sweep
kind: new
title: "Tracker reconciliation: close resolved, obsolete and duplicate issues with evidence, drop stale blocking edges, and re-home orphans"
priority: P2
type: chore
labels: [agents, beads, governance, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Tracker reconciliation: close resolved, obsolete and duplicate issues with evidence, drop stale blocking edges, and re-home orphans

## Problem

The tracker lags reality:

- Issues whose premise no longer holds stay open, and their stale blocking
  edges keep other issues out of `bd ready`.
- Duplicates stay open.
- Open children sit under closed parents.

This issue is **tracker-only** work: closes, edge removals and re-parents,
each with evidence. It needs no branch. The recurring detector (a
landed-not-closed drift-patrol check) is a separate code change:
landed-not-closed-patrol-check.

## Ordering

None. (str-qwua7.1 was closed on 2026-09-24 with its git-state check landed, so there is no
re-scope note to post first. The identity part of that check is the new issue
`qwua7-1-git-state-check`.)

## Evidence (re-verified 2026-09-23 with `bd show`; verify each again before acting)

| Issue | State now | Evidence | Action |
|---|---|---|---|
| str-qwua7.1 (P1, "Repair primary checkout (core.bare=true) and add a git-state hygiene check") | open | `git -C /home/ketan/project/shatter config core.bare` → `false` | **Do not close.** Only remove its stale `blocks` edges to .18 and .19. |
| str-qwua7.18 (P1, "Land the six parked worktree branches in order; close str-duens and remove its worktree") | open; `bd dep list` shows `str-qwua7.1 ... via blocks` | str-rmcrl, str-na9db, str-0z1im, str-6vl7p, str-vr7vq, str-8q1b4 and str-duens are closed in the live DB; `git worktree list` shows no duens or 8q1b4 worktree | Close. The five merged remote branches (`origin/str-{0z1im,6vl7p,8q1b4,rmcrl,vr7vq}-*`) are left to cleanup-merged-remote-branches.sh once beads-jsonl-consumers-drop-bd-sync fixes its in-progress source. |
| str-qwua7.19 (P1, "Push local main ... and prune the land-work preview worktrees") | open, blocked by str-qwua7.1 | main == origin/main; `git worktree list \| grep -c land-work-preview` → 0 | Close. |
| str-mpgg1 | in_progress since 2026-09-01 | 84941b37 is an ancestor of origin/main | Closed by mpgg1-close; do not close twice. |
| str-qe9pp (P2, "Rust frontend parity/conformance drift: golden + prepare timeout") | open, last updated 2026-07-06 | golden updated in 750b7ff8 (2026-06-18); prepare timeout fixed by str-uoclg (closed 2026-08-13) | Re-run the check the issue names (Rust conformance/parity gate) on current main; close with that output plus both citations if green, otherwise comment with the failure. |
| str-uj3y (P2, "build-frontend rust should vendor shatter-rust-runtime beside the custom binary ...") | open | Duplicates str-da35 (closed, landed 4182e51e; `build_frontend.rs` vendors the runtime) | Close as a duplicate of str-da35 after confirming the vendoring code is on origin/main (`git grep` output in the reason). |
| Orphans: str-qwua7.56.1, str-qwua7.9.1, str-hy9b.J3, str-hy9b.1 | open under closed parents | `audits/2026-09-22/gates/drift-patrol.log:35-39` | Re-parent each under a live epic, or close it, with a reason. |

**Not in this sweep:**

- **str-qwua7.12** (SPEC §2.11 exit codes) is **not** closed. Single-target
  error classes now exit 2, but its multi-target acceptance check (exit 1 on
  partial failure) contradicts current code and its test
  `decide_exit_status_ok_partial_success_with_some_failed_targets`
  (`shatter-cli/src/commands/explore.rs:7075`). It gets a re-scoping note
  instead: exit-codes-qwua7-12-note (bucket shatter-cli-flags-and-help).
- **str-1fik / str-wfd2, str-hrg2 / str-0wxw, str-2zsy, str-cl53,
  str-u394l.4:** these are str-qwua7.62's approved 2026-09-06 decisions and
  stay with str-qwua7.62. The audit reviewer (frontend-rust-10) notes that
  wfd2 is the cross-crate item and same-crate synthesis was delivered by
  str-do53; whoever executes .62 should give the survivor a body saying
  "same-crate resolved (str-do53); cross-crate still Opaque".
- str-8q1b4 was closed on 2026-09-22 and needs nothing.

Findings: agent-repo-05, sessions-09, prior-09, protocol-parity-16,
frontend-rust-10 (tracker part). Source draft:
`drafts/shatter-agent/10-tracker-reconciliation-sweep.md`.

## Acceptance criteria

- [ ] Every row in the table ends closed, re-scoped (edges removed) or
      re-parented, or carries a comment explaining why not. Each close reason
      cites its evidence (command plus output, or a SHA). No bare "Closed"
      reasons.
- [ ] `bd dep list str-qwua7.18` and `bd dep list str-qwua7.19` no longer
      show `str-qwua7.1`, or both issues are closed; the output is in the
      close reason.
- [ ] `bd show str-qwua7.12` is still open after this sweep.
- [ ] `python3 scripts/drift-patrol.py --only tracker-hygiene`, run against
      live bd after the sweep, shows no orphan from the table and no stale
      claim except ones with a documented reason; output in the close reason.

## Out of scope

- The landed-not-closed patrol check (landed-not-closed-patrol-check).
- The priority rubric, P1 cap and epic waves (triage-policy-and-audit-epic-waves).
- Executing str-qwua7.62's decisions.
- Deleting remote branches (cleanup script, after beads-jsonl-consumers-drop-bd-sync).
- Re-verifying str-qwua7.14 (qwua7-14-reverify-on-main, bucket shatter-agent-guidance-and-repo-hygiene).

## Priority / type / labels

P2, chore. Labels: agents, beads, governance, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: none.
- Related: mpgg1-close, exit-codes-qwua7-12-note, qwua7-17-drift-patrol-hygiene,
  landed-not-closed-patrol-check, str-qwua7.62,
  beads-jsonl-consumers-drop-bd-sync.
