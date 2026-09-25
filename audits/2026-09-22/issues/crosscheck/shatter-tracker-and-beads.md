# Cross-check review

- **Reviewer:** claude (DEGRADED same-runtime fallback)
- **Artifact type:** issue
- **Mode:** degraded

> **DEGRADED REVIEW.** The counterpart runtime was unavailable, so this review came from an independent agent of the *same* runtime. It shares the original author's model and blind spots; weight it accordingly.

## Findings

# Cross-check review (DEGRADED: same-runtime fallback) -- bundle shatter-tracker-and-beads

Reviewer: Claude (same runtime as the author). The Codex review failed identity
validation (exit 4), so this is the fallback review, done read-only against
/home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 and
/home/ketan/project/shatter on 2026-09-23.

## What I verified (all matched the drafts)

- Line references: AGENTS.md:125, :293, :340, :345-375 ("Beads Sync Cadence",
  "Leave the managed git hooks alone"), :270-280 (cleanup reads the JSONL);
  `.claude/skills/audit/SKILL.md:380-387`; `.beads/PRIME.md:8`;
  `scripts/drift-patrol.py:188-221` and `:523 check_tracker_hygiene`;
  `docs/DRIFT-PATROL.md:53-55`; `scripts/cleanup-merged-remote-branches.sh:15,27,105-110`.
- Git: only `audit-2026-09-22` and `origin/audit-kapow-2026-05-24` audit refs
  exist. `e067979d`, `42c112cd` and `f9dad247` exist as objects. No reflog
  entry holds them, so they really are unreachable. The 09-04 chain has 7
  commits on top of 84941b37. `134dd616 2026-09-07` is the last JSONL commit;
  the JSONL has 1,733 lines. `84941b37` is an ancestor of origin/main.
  `core.bare=false`. The five `origin/str-{0z1im,6vl7p,8q1b4,rmcrl,vr7vq}-*`
  branches exist. origin/main `audits/` and `docs/audits/` match what the
  drafts say.
- bd: version 1.1.0. `import.auto = true (default)`, `backup.git-push = true`,
  `export.auto = false`. The Dolt remote `origin` exists and
  `push-state.json` has last_push 2026-04-11. The hook text matches
  (v0.63.3, `${BEADS_HOOK_TIMEOUT:-300}`). Live DB has 1,778 issues. Open by
  priority is P1 45 / P2 112 / P3 23 / P4 1 (181; 182 with in_progress).
  str-qwua7 has 62 direct children: 12 closed, 50 open. There are 12 open
  `agents`-labelled issues.
- Issue states, all as the drafts claim: qwua7.17 closed 09-08; qwua7.28
  open P2; ly5bz open P3; mpgg1 in_progress P1 (last updated 09-01); 8q1b4
  closed 09-22; qe9pp and uj3y open; da35, uoclg and do53 closed; duens and
  na9db closed; 1fik (P1) and wfd2 (P2) open; the four orphans are open;
  35vtk.10 is P1 and 35vtk.29/.31 are P2. str-qwua7.62 records "merge
  str-wfd2 into str-1fik (P1) and close str-wfd2". The downstream levers
  la75, jyxr, 4yc9w and bh9wu all exist and are open.
- `grep BEADS_HOOK_TIMEOUT scripts/ .beads/hooks Taskfile.yml` finds nothing,
  which supports draft 04's stale-fact claim.

Not independently re-run: the str-qwua7.12 exit-code reproductions, the
`bd dep list` edges on qwua7.18/.19, and the 20-status/12-text mismatch lists.

## Findings

### MAJOR

1. **[tracker-reconciliation-sweep] MAJOR: The blocked-by target is a comment, not an issue, so the filer cannot create it and the likely mapping blocks forever.**
   `blocked_by: [qwua7-1-git-state-check]` names a note-to-existing
   (a comment on str-qwua7.1), so it has no issue id of its own. If the filer
   resolves it to its `existing_id` (str-qwua7.1), this sweep becomes blocked
   by the re-scoped, still-open git-state-check implementation. That is
   exactly the stale-edge problem the sweep is meant to remove. The draft
   should state the ordering in prose ("post the qwua7.1 note first") and drop
   the dependency edge.

2. **[tracker-reconciliation-sweep] MAJOR: One issue carries at least three separate jobs.**
   It covers eight-plus evidence-backed tracker closes and re-parents, running
   every outstanding str-qwua7.62 decision (hrg2/0wxw, wfd2/1fik, 2zsy, cl53,
   u394l.4 priority), and a new drift-patrol check with unit tests that must
   land through land-work. The tracker-only closes and the code change have
   different workflows and different done-points. Split the patrol check into
   its own issue, and leave the .62 execution on .62 itself, which already has
   an approved decision.

3. **[beads-retire-jsonl-import-dolt-remote] MAJOR: The hard `< 15 s` / `< 30 s` targets rest on an unproven cause.**
   The evidence says the import uses about 10 s of CPU and spends the rest
   "waiting, not computing". Nothing identifies what it waits on: Dolt server
   start, a lock held by a sibling worktree, or network. Disabling
   `import.auto` fixes the stall only if the wait is inside the import path.
   bd's `config --help` describes `import.*` only as "JSONL import settings",
   not as the switch for the hook. Add a first criterion that finds the wait
   (for example with strace or bd debug logging). Make the `< 15 s` target
   conditional on that result, or say what happens if the remaining latency
   is not the import.

4. **[beads-jsonl-import-clobber-check / beads-retire-jsonl-import-dolt-remote] MAJOR: The durations contradict each other.**
   Drafts 01, 02 and 04 say the hook "spends about 6 minutes" importing, but
   the hook runs under `timeout 300`. Draft 02 also cites
   `time bd hooks run post-checkout` = 2m59.7s, and land.py shows
   234.9-301.2 s. A 6-minute run through the hook cannot happen. State where
   each number came from (direct `bd hooks run` vs. hook-wrapped) and use one
   baseline. There is also a clobber-relevant consequence the drafts miss:
   when the timeout kills an import mid-write, the import may be left partly
   applied. Draft 01's history scan should look for that case explicitly.

5. **[publish-audit-reports] MAJOR: Duplicates str-qwua7.22 and bundles recovery, forensics, two landings and a skill rewrite.**
   str-qwua7.22 (open, "Rewrite /audit Phase 10 to be worktree- and
   tracker-safe") already owns the SKILL.md landing rewrite. The draft only
   says "coordinate". It also mixes a time-critical ref recovery, which should
   happen now and not be an acceptance criterion on a P1 ticket waiting in a
   queue, with a root-cause hunt for the branch deletion, two report landings
   (one needing a privacy review of `sessions/` transcripts), and an INDEX
   section. Decide now: either close .22's Phase-10 part into this issue or
   move the skill criterion to .22. Also have the maintainer create
   `audit-2026-09-04-recovered` before filing, as the bundle's reviewer note
   already asks.

### MINOR

6. **[publish-audit-reports] MINOR: The file count is off by one.** `git ls-tree -r e067979d -- audits/2026-09-04` has
   97 files, not 98, plus `audits/2026-09-04.md`.
7. **[publish-audit-reports] MINOR: The target path reads .44 backwards.** str-qwua7.44 says root `audits/`
   merges into `docs/audits/` *after* the 09-04 branch lands at `audits/`.
   The draft makes `docs/audits/2026-09-04/` the primary target. Make
   `audits/2026-09-04/` the default and let .44 move it.
8. **[beads-jsonl-import-clobber-check] MINOR: The "stop new damage" step is not practical.** Step 1 says not to
   create worktrees until draft 02 lands, but 02 is blocked by 01, and every
   landing creates a preview worktree. Say plainly that damage will continue
   during the investigation and that the history scan's upper bound is the
   moment the import is disabled.
9. **[beads-jsonl-consumers-drop-bd-sync] MINOR: The new docs check needs bd in CI, which the draft says CI lacks.** The
   docs check that runs `bd <sub> --help` depends on bd being installed. The
   draft establishes that CI has no bd, so in CI the check will SKIP or fail.
   Say where it runs, or check against a pinned subcommand list.
10. **[triage-policy-and-audit-epic-waves] MINOR: The new inversion FAIL check goes red immediately.** A
    blocked-by-lower-priority FAIL fails on day one because of
    str-35vtk.10 → .29/.31, and there may be more. Either fix the known
    inversions first as a criterion, or land the check as WARN. The draft is
    also large (rubric, re-triage to at most 15 P1s, epic split, three patrol
    checks, a WIP rule) and could be split into policy and patrol-check
    issues.
11. **[downstream-coverage-goals-epic] MINOR: Some evidence lives outside the repo.** The memory files are per-user
    and outside the repo, so a fresh agent on another machine cannot read
    them. Quote the baseline numbers and the recipe inline in the body (the
    body currently gives only the percentages).
12. **[qwua7-17-drift-patrol-hygiene] MINOR: It adds little.** It is a comment on a closed issue,
    and its action items live in other drafts. It could be dropped.

## Verdict

The bundle is factually solid. Every file:line reference, SHA, tracker state
and count I checked matches reality, and the D4 sequencing (01 → 02 → 03) is
coherent. The close/note drafts (04, 05, 06, 11) are ready to file as written.
Drafts 01, 03 and 10 are ready after minor edits. Drafts 02, 07 and 08 need
changes first.

Top fixes:
1. Draft 08: replace the dependency on the qwua7-1 note with a written
   ordering, and split the drift-patrol code change (and the .62 execution)
   out of the tracker sweep.
2. Drafts 01/02/04: settle on one latency baseline, and make 02's first
   criterion find what the import is waiting on before committing to
   `< 15 s`. Add partial imports cut off by the timeout to 01's scan.
3. Draft 07: resolve the overlap with str-qwua7.22, and have the maintainer
   create the 09-04 recovery ref before filing instead of leaving it as a
   ticket criterion.
