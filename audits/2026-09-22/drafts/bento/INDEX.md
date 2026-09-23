SUPERSEDED by ../../issues/INDEX.md — do not run file.sh here

# Bento issue drafts: shatter audit 2026-09-22

The target tracker is beads in `/home/ketan/project/bento` (prefix `bento-`), and the filer is `file.sh`, which has not been run. Each draft has a metadata header, then `---BODY---`, then the body that file.sh passes via `--body-file`. Source code facts were checked against bento `1c0c1e6` (catalog/ paths are canonical; plugins/ is generated).

The readiness check was a local fallback. No subagent or Task tool was available in this run, so the drafts follow the issue-readiness-check completeness template but have not had a fresh reviewer. **Run bento:issue-readiness-check with a fresh reviewer on each draft before filing.** file.sh pauses for confirmation.

Selection: findings with target_repo=bento, not refuted, and with a dedupe relation of new, partially-covered or duplicate-closed-but-unfixed. Findings marked "related" count as new: their dedupe notes say no existing issue covers them.

## Drafts (new issues, children of the epic)

| # | Title | Pri | Type | Source findings | Relation |
|---|---|---|---|---|---|
| 00 | Epic: Audit 2026-09-22 findings (bento) | P1 | epic | all | — |
| 01 | Git guard: close /usr/bin/git, wrapper-prefix, -C, cd, GIT_CONFIG bypasses; stop quoted/heredoc false positives | P1 | bug | sessions-02, bento-04, sessions-15 | follow-up to closed-unfixed bento-rdtn.15 |
| 02 | land.py reports verifier.log as output_path after deleting it with the preview | P1 | bug | bento-02 | follow-up to closed-unfixed bento-rdtn.4/.14 |
| 03 | Budget and surface beads git-hook latency; guard's slow-hook pointer cites empty reference | P1 | bug | bento-03 | new |
| 04 | land-work: report GitHub workflow conclusions for landed SHA; doctor flags red workflows | P1 | feature | tests-ci-03 (bento part) | new (rel. bento-1qry) |
| 05 | check-unpushed: count vs all remotes, skip unowned checkouts, don't block during own land.py | P2 | bug | bento-11, sessions-10 | partially-covered (bento-neng closed) |
| 06 | land.py aborts/resets merge state in shared primary it did not start | P2 | bug | bento-05 | new |
| 07 | land.py: canonical long-running invocation; progress log + heartbeat | P2 | feature | bento-06 | new |
| 08 | Doctor/guard .agent-mode.local is per checkout; linked worktrees never collapse | P2 | bug | bento-08 | closed-unfixed bento-rdtn.2 |
| 09 | Stale previews: test-suite /tmp leak, TMPDIR ignored, unscoped doctor, nonexistent closure mode | P2 | bug | bento-09, sessions-17 | new (rel. rdtn.1, 7n7) |
| 10 | closure: apply mode for orphan worktree dirs | P2 | feature | bento-10 | new (rdtn.1 deferred it) |
| 11 | land-work: delete landed remote + superseded same-issue branches, verified | P2 | feature | prior-06, bento-15 | partially-covered |
| 12 | Close flow: reject bare "Closed"; verify cited SHAs are ancestors of primary | P2 | feature | prior-13 | partially-covered (1qry, 1bl, v57) |
| 13 | Reconcile claims ↔ branches (landed-not-closed fails; in_progress w/o branch reported) | P2 | feature | prior-10, bento-17 | partially-covered (rdtn.8/.9) |
| 14 | Verifier contract drift: warn on missing `executed`; require output pass-through | P2 | feature | bento-13 | partially-covered (rdtn.6) |
| 16 | land-work SKILL.md restructure around land.py; fix $(...) contradictions | P2 | task | bento-14 | partially-covered (bento-by8) |
| 17 | beads-issue-flow: jsonl snapshot + Dolt remote procedure for bd 1.x; doctor checks | P2 | task | bento-16 | new (rel. rdtn.12) |
| 19 | land.py: make --require-up-to-date a repo policy option | P3 | feature | bento-07 | new |
| 20 | land.py merge_push: timed sub-steps + pre-push hook stderr | P3 | feature | bento-18 | new |
| 21 | Merge-message template; stale pushed-branch nudge; one-session-per-branch guidance | P3 | feature | agent-repo-17 | partially-covered (rdtn.9/.14) |
| 22 | Follow-ups as siblings w/ discovered-from; guard closing parents with open children | P3 | feature | prior-11 | new (bento side) |
| 23 | swarm/audit prompt templates: per-subagent scratch subdirectory | P3 | task | cli-ux-20 | new (verifier questioned target; scoped to bento templates) |

## Notes to append to existing issues (no new issue)

| File | Existing issue | Source finding | Why |
|---|---|---|---|
| 15-swarm-lead-lands-from-teammate-worktree.md | bento-eth (open, P3) | bento-12 | partially-covered: add "land from lead-owned scratch worktree" + `land.py --branch` |
| 18-admission-control-hooks-and-land.md | bento-dyp7 (open epic, P1) | sessions-07 | partially-covered: hooks + land.py through governor, "machine busy" line |
| 90-note-bento-e583.md | bento-e583 (open, P1) | bento-01 | duplicate-open: confirmed mechanism + owner-lock acceptance |
| 91-note-bento-a0nz.md | bento-a0nz (open, P1) | core-23 | duplicate-open: behavioural-probe/caller-check requirement + Rust reachability |

## Skipped: duplicate-open (listed per instructions; notes 90/91 above are optional comments)

- bento-01 → **bento-e583**
- core-23 → **bento-a0nz** (also related to bento-m4en, bento-r85d, bento-y993)

## Suggested dependency edges (applied by file.sh)

- 02 related to 07 (both change land.py's log handling). Implement the log location once.
- 12 blocks 22 (the close helper is reused for the open-children guard).
- 09 related to bento-e583 (closure's preview removal must honour the owner lock).
- 05 related to 07 (land.py's in-progress marker).
- 20 related to 07 (shared progress log).

## Note from the completeness review

- 04 is P1 here, but the report (§14 bento P2, §15 #40) lists tests-ci-03 as P2. The area evidence (tests-ci T-03) says P1. Reconcile before filing; see §15.1 of `audits/2026-09-22.md`.
