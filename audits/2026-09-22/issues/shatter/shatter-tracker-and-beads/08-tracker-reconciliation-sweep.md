---
slug: tracker-reconciliation-sweep
kind: new
title: "Tracker reconciliation: close resolved, obsolete and duplicate issues with evidence, and add a landed-not-closed drift-patrol check"
priority: P2
type: chore
labels: [agents, beads, drift, governance, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [qwua7-1-git-state-check]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Tracker reconciliation: close resolved, obsolete and duplicate issues with evidence, and add a landed-not-closed drift-patrol check

## Problem

The tracker lags reality in several ways:

- Issues whose premise no longer holds stay open, and their stale blocking
  edges keep other issues in `bd ready`.
- Landed work stays `in_progress`.
- Duplicates stay open.
- Work done under a different issue id never touches the umbrella issue.

Nothing detects "branch merged but issue still open or in progress", so these
cases pile up until an audit finds them.

## Evidence (re-verified 2026-09-23; verify each again before acting)

| Issue | State now | Evidence | Action |
|---|---|---|---|
| str-qwua7.1 (P1, "Repair primary checkout (core.bare=true) and add a git-state hygiene check") | open | `git -C /home/ketan/project/shatter config core.bare` gives `false` | **Do not close.** qwua7-1-git-state-check re-scopes it to the git-state check (D5). Only remove its stale `blocks` edges to .18 and .19. |
| str-qwua7.12 (P1, SPEC §2.11 exit codes) | open | At HEAD, `explore nope.ts:foo`, an unknown function, the sandbox refusal and a bad-file `spec-diff` all exit 2. `error_exit_code` dates from 464e3c9e, an ancestor of the 09-04 audit base, so the finding came from a stale binary (prior-09). | Close it with that evidence. Move the remaining sub-item (SPEC.md:634 names a nonexistent `--failure-threshold`) to a new small issue, or to the SPEC-drift work under str-wurp. |
| str-qwua7.18 (P1, "Land the six parked worktree branches ...; close str-duens ...") | open, `blocked_by` str-qwua7.1 (`bd dep list`) | str-rmcrl, str-na9db, str-0z1im, str-6vl7p, str-vr7vq, str-8q1b4 and str-duens are all closed in the live DB. `git worktree list` shows no duens or 8q1b4 worktree. | Close. Five merged remote branches remain (`origin/str-{0z1im,6vl7p,8q1b4,rmcrl,vr7vq}-*`); leave them to cleanup-merged-remote-branches.sh once beads-jsonl-consumers-drop-bd-sync fixes its in-progress source. |
| str-qwua7.19 (P1, "Push local main ... and prune the land-work preview worktrees") | open, `blocked_by` str-qwua7.1 | main == origin/main. `git worktree list \| grep -c land-work-preview` gives 0 (the `/tmp/land-work-preview-a5l9ycto` seen on 09-22 is gone). | Close. |
| str-mpgg1 | in_progress since 2026-09-01 | 84941b37 is an ancestor of origin/main | Close via mpgg1-close (same run; do not close twice). |
| str-qe9pp (P2, "Rust frontend parity/conformance drift: golden + prepare timeout") | open, last updated 2026-07-06 | golden updated in 750b7ff8 (2026-06-18); prepare timeout fixed by str-uoclg (closed 2026-08-13) | Close, citing both. |
| str-uj3y (P2) | open | Duplicates str-da35 (closed, landed 4182e51e; `build_frontend.rs` vendors the runtime) | Close as a duplicate of str-da35. |
| str-1fik (P1, empty body) / str-wfd2 (P2) | both open | Both ask for cross-file/cross-crate Rust type synthesis. str-qwua7.62 (approved 2026-09-06) decided to merge wfd2 into 1fik. The audit reviewer (frontend-rust-10) suggests the reverse: wfd2 is cross-crate synthesis, and same-crate was delivered by str-do53. | Follow the recorded .62 decision unless the maintainer overrides it. Either way the survivor gets a body that says "same-crate resolved (str-do53); cross-crate still Opaque". |
| Orphans: str-qwua7.56.1, str-qwua7.9.1, str-hy9b.J3, str-hy9b.1 | open under closed parents | `audits/2026-09-22/gates/drift-patrol.log:35-39` | Re-parent (for example under the 2026-09-22 epic or a live epic) or close each, with a reason. |

str-8q1b4 (flagged in the report) was closed on 2026-09-22 and needs no
action here.

Findings: agent-repo-05, sessions-09, prior-09, protocol-parity-16,
frontend-rust-10 (tracker part only; the memory note was corrected on
2026-09-23). The source draft is
`drafts/shatter-agent/10-tracker-reconciliation-sweep.md`. Related:
prior-12, which records that str-qwua7.62's approved decisions are still
unexecuted after 16 days (str-hrg2 → str-0wxw duplicate close, wfd2/1fik
merge, str-2zsy and str-cl53 bodies, str-u394l.4 → P1).

## Acceptance criteria

- [ ] Every row in the table is closed, re-scoped or re-parented. Each close
      reason cites the evidence (command plus output, or a SHA). No bare
      "Closed" reasons.
- [ ] `bd dep list str-qwua7.18` and `bd dep list str-qwua7.19` no longer
      show `str-qwua7.1`, or both issues are closed.
- [ ] str-qwua7.62's approved decisions are executed in the same session, or
      a note on .62 explains why each remaining one is deferred.
- [ ] A note appended to str-qwua7.62 points to this issue for the items
      above.
- [ ] `python3 scripts/drift-patrol.py --only tracker-hygiene`, run against
      live bd, passes or lists only items with a documented reason. The output
      is in the close reason.
- [ ] New drift-patrol check, added in `scripts/drift-patrol.py` next to
      `check_tracker_hygiene` (:523):
      - **FAIL** for an `in_progress` or `open` issue whose branch (`<id>-*`
        or `<id>`, local or `origin/`) is an ancestor of origin/main
        (landed but not closed).
      - **WARN** for an `in_progress` issue older than 14 days with no
        matching local or remote branch or worktree.
      - Unit tests with canned bd and git data cover FAIL, WARN and clean
        cases.
- [ ] The patrol check lands through launch-work/land-work, with `task
      affected` green and its `Gates selected` line in the close reason.

## Suggested approach

The closes are tracker-only work and need no branch. The patrol check needs
a branch. Run the closes after qwua7-1-git-state-check has been posted, so
.1 is re-scoped rather than closed.

## Out of scope

- The priority rubric, P1 cap and epic waves (triage-policy-and-audit-epic-waves).
- Implementing Rust cross-crate type synthesis.
- Deleting remote branches (cleanup script, after beads-jsonl-consumers-drop-bd-sync).
- Re-verifying str-qwua7.14 (fixture-corruption-incident-reverify).
- Memory files (already corrected 2026-09-23).

## Priority / type / labels

P2, chore. Labels: agents, beads, drift, governance, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: qwua7-1-git-state-check (the note re-scoping str-qwua7.1, in
  bucket shatter-agent-guidance-and-repo-hygiene).
- Related: mpgg1-close, qwua7-17-drift-patrol-hygiene, str-qwua7.62,
  beads-jsonl-consumers-drop-bd-sync.
