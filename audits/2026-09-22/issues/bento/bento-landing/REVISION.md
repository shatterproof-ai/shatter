# Revision: bento-landing (2026-09-23)

Inputs: Codex cross-check `issues/crosscheck/bento-landing.codex.md` (primary) and the same-runtime review `issues/crosscheck/bento-landing.md` (secondary). The Codex reviewer could not query live tracker state. Duplicate/overlap claims were checked here with `bd show` in /home/ketan/project/bento (bento-73de, bento-x4bm, bento-49pg, bento-eth, bento-qiw, bento-i76i, bento-2jo, bento-e583, bento-dyp7). Code claims were re-checked against bento origin/main 0b8d488 (land-work, swarm, wire-land-verifier and closure unchanged since b1bb787/1c0c1e6). Nothing is filed (D6).

## Codex findings

| # | Sev | Finding (one line) | Action | Files |
|---|---|---|---|---|
| 1 | BLOCKER | 07: remote deletion races with pushes after the ancestry check; `--superseded` by issue id is unsafe | **Applied.** `bd show bento-73de` confirms an open P2 issue that already specifies an exact-ref fetch, ancestry check on that SHA, and a `--force-with-lease=<ref>:<sha>` delete with a lease-rejected test. 07 converted to a note on bento-73de (slug kept). The draft's own deletion design was dropped. `--superseded` is now report-only (unique-commit counts via `git cherry` plus worktree ownership), never auto-delete. | 07 |
| 2 | MAJOR | 01: owner rule would refuse land.py's own cleanup subprocess | **Applied.** Confirmed `land.py:129-142` runs `--cleanup` as a child. Added owner token in `owner.json`, handed to the child via `--owner-token` or `BENTO_LAND_OWNER_TOKEN`; foreign cleanup refused without a token; tests for owner cleanup succeeding while the lock is held, foreign refusal, and `--force-foreign`. | 01 |
| 3 | MAJOR | 05: "attempted merge" flag does not prove ownership | **Applied.** Ownership now comes from a `primary-checkout.lock` flock taken before the ff/merge, a no-`MERGE_HEAD` precondition under the lock, `MERGE_HEAD == merged feature head` before abort, and a parent-check before reset. The residual window for non-bento writers is documented. | 05 |
| 4 | MAJOR | 05: regression setup cannot exercise the claimed failures | **Applied.** Confirmed prepare refuses a dirty primary (`land-work-prepare.py:117-121`) and `BENTO_LAND_TEST_DELAY_MERGE` sleeps before `git merge` (`land.py:187-192`). Tests now inject the foreign merge after prepare, pause while `MERGE_HEAD` exists (asserted before SIGINT), and inject a concurrent HEAD move before reset. | 05 |
| 5 | MAJOR | 02/03: existing tail covers only killed/timeout | **Applied.** Confirmed at `land-work-run-verifier.py:136-138`. 02 now widens `_fail()` to ordinary failures, and its regression stub emits valid failing JSON with an assertion on `verifier_status == "failed"`. 03's wording corrected. | 02, 03 |
| 6 | MAJOR | 04: polling can report completion before runs appear | **Applied.** Separate discovery and settling phases, a distinct `no_runs` status, `--limit 100` with a truncation flag, and tests for initially-empty-then-appearing and late-appearing runs. | 04 |
| 7 | MAJOR | 04: doctor approach contradicts its call budget | **Applied.** One batched `gh run list --branch --status completed --limit 200` call, grouped client-side. Cancelled/skipped runs are ignored in the streak; cache test added. | 04 |
| 8 | MAJOR | 07: preview-cleanup verification already implemented | **Applied.** Confirmed `land.py:321-330` passes `--preview-dir` to verify-landing and the driver tests assert no registered preview after success/failure. The claim and the criterion were removed. Failure-path cleanup failures are already reported as `cleanup: failed`, so no replacement criterion was added. | 07 |
| 9 | MAJOR | 08: regenerated wrappers also omit `executed` | **Applied.** Confirmed at `wire-land-verifier.py:893-917` (go-task only). 08 now defines `cache_status: "unknown"` for non-task checks, gives the warning a meaningful target, and adds a regenerated-wrapper endpoint test that must produce no warning. | 08 |
| 10 | MAJOR | 09: shortened serial path could drop work land.py does not do | **Applied.** Evidence lists the obligations land.py skips (hooks, gate baseline, review, tracker, root hygiene, teardown). The acceptance criteria require an ordered checklist kept in SKILL.md, a step-mapping table at close, and lint on the required step names. | 09 |
| 11 | MAJOR | 09: `$(` lint expands to every skill | **Applied.** Lint scoped to `catalog/skills/land-work/`. Other skills are listed as out of scope and need a separate inventory issue. | 09 |
| 12 | MAJOR | 10: addendum exceeds bento-eth's scope | **Applied (split).** `bd show bento-eth` confirms "Out of scope: land-work's own mechanics". The driver change moved to the new issue `land-py-branch-flag` (15). 10 is now swarm-template text only and is ordered after 15. | 10, 15 |
| 13 | MAJOR | 11: nested governor acquisition can deadlock | **Applied.** Added reservation handoff (a lease token exported to children and honoured only while live), plus nested-saturation and stale-token tests. | 11 |
| 14 | MAJOR | 14: bundles independent features; bare-SHA ban conflicts with `merge --ff-only <leased_sha>` | **Applied (split plus drop).** 14 now covers only the merge message (slug kept). New `stale-pushed-branch-doctor-nudge` (16) and `one-session-per-branch-guidance` (17). The bare-SHA guard was dropped: bento-rdtn.15 (5210e74, 2026-09-10) already denies every Bash `git merge` in the primary checkout, all 6 observed bare-SHA merges predate it (2026-09-07/08), and land.py's subprocess merges are invisible to the guard. | 14, 16, 17 |
| 15 | MINOR | 05: fast-forward alone does not make `HEAD@{1}` wrong | **Applied.** Claim corrected; the ff-then-mismatch test replaced by a concurrent-HEAD-move test. | 05 |

## Same-runtime review findings (secondary)

| Sev | Finding | Action | Files |
|---|---|---|---|
| BLOCKER | 07 duplicates bento-73de | Applied (same as Codex #1). | 07 |
| MAJOR | 02 overlaps bento-x4bm; log locations conflict | **Applied.** `bd show bento-x4bm`: it passes `--log` only with `--issue/--closure-note` and is blocked by 4 issues. 02 stays a new P1 bug (unconditional persistence plus ordinary-failure tail), adopts x4bm's `<git-common-dir>/bento/landing/` root, and carries a companion comment for bento-x4bm. 06's progress log moved to the same directory. | 02, 06 |
| MAJOR | 02 tail criterion assumes nonexistent behaviour | Applied (Codex #5). | 02 |
| MAJOR | 03 should cite x4bm | Applied. | 03 |
| MAJOR | 09 contradicts bento-49pg Option C | **Applied.** `bd show bento-49pg` confirms Option C ("one rule in land-work and beads-issue-flow"). The Tracker Handoff criterion now carries 49pg's single rule, and 49pg is linked as land-first. | 09 |
| MINOR | 09 lint scope | Applied (Codex #11). | 09 |
| MINOR | 05 HEAD@{1} reasoning | Applied (Codex #15). | 05 |
| MINOR | 06 exit-code line and 14-vs-36 count | **Applied.** Exit code cited at `land.py:383`. The transcript was recounted by parsing `tool_use` inputs: 20 invocations and 40 raw occurrences (the transcript had grown since the audit). | 06 |
| MINOR | 04 link bento-2jo | Applied. | 04 |
| MINOR | 08 "land.py ignores run-verifier warnings" | Applied. Both sides are now stated as new (`warnings` key, `verify_warnings`). | 08 |
| MINOR | 10 "bento-qiw settles who lands" | Applied. The text now cites the recorded 2026-06-12 operator decision on qiw/eth. | 10 |
| MINOR | 14 link bento-i76i | Moot. The bare-SHA guard was dropped; 14 points remaining guard gaps to bento-i76i. | 14 |
| MINOR | stale bundle SHA | Applied. Now 0b8d488. | all, BUNDLE.md |

## Disputed

None. Every Codex finding was confirmed against the code or the tracker.

## Splits and conversions

- **07 `landing-deletes-remote-branches`:** new → note-to-existing on bento-73de. The slug is kept. `file-all.sh`'s `BLOCKER_SLUGS` hold for this slug can be lifted once the maintainer accepts the conversion.
- **10 `eth-swarm-lead-lands-from-teammate`:** narrowed. The driver work moved to **15 `land-py-branch-flag`** (new). 10 has `blocked_by: [land-py-branch-flag]`, which is ordering only, because its comment cites the new id.
- **14 `merge-message-and-stale-branch-nudge`:** narrowed to the merge message. The slug is kept even though its name still mentions the nudge. New **16 `stale-pushed-branch-doctor-nudge`** and **17 `one-session-per-branch-guidance`**. The bare-SHA guard was dropped.
- **03 `verifier-log-reopen-note`:** now `blocked_by: [land-py-verifier-log-kept]` (ordering for the placeholder).
- **02:** gains a `## Comment for bento-x4bm` companion section.
