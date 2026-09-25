# Revision: shatter-tracker-and-beads (2026-09-23)

Inputs: `crosscheck/shatter-tracker-and-beads.codex.md` (primary, 13 findings)
and `crosscheck/shatter-tracker-and-beads.md` (degraded same-runtime review,
secondary). Codex could not query bd; every tracker claim below was checked
with `bd show` / `bd dep list` in /home/ketan/project/shatter on 2026-09-23.

## Codex findings

| # | Sev | Finding (one line) | Action | Files |
|---|---|---|---|---|
| 1 | BLOCKER | Draft 07 creates a filing prerequisite cycle (publication must precede the filer but is itself unfiled and depended on draft 02's Dolt procedure) | **Applied.** 07 narrowed to landing the 2026-09-22 report only, with no dependency on any unfiled issue (no Dolt procedure, no `bd sync`). Explicit 4-step maintainer bootstrap: recovery refs now; file only epic + 07 (`ONLY=shatter`, `HOLD` others, or by hand + ledger row); land 07; then bulk filer. | 07, BUNDLE.md |
| 2 | MAJOR | Draft 01 equates historical reversions with corruption | **Applied.** Candidate detection separated from confirmation; each candidate records the reverting commit's author/time; classification `intended` / `import damage` / `unresolved`; maintainer approves the repair list; only `import damage` is repaired. Added a scan self-test against a seeded scratch clone so "0 candidates" cannot come from a broken scan. | 01 |
| 3 | MAJOR | Draft 02 does not prove import retirement across all entry points or fresh clones | **Applied.** Verified `.git/hooks/post-merge` carries the same beads block. New diagnosis issue 12 lists every import entry point (hooks + command-time auto-import) from bd source; 02 requires per-entry-point proof (no "importing JSONL" line and no new import-authored Dolt commit) and a fresh-clone proof with a documented bootstrap plus a failing doctor check when it is skipped. | 02, 12 |
| 4 | MAJOR | Draft 03's zero-`bd sync` condition contradicts its str-qwua7.23 exclusion | **Applied.** 03 now owns every line-level `bd sync` removal (including AGENTS.md:125 and :345-369); qwua7.23 keeps only the landing-section rewrite. This matches the existing qwua7-23-agents-md-rtk-and-landing note in the other bucket, which already assigns line-level removal to 03. No ordering edge needed. | 03 |
| 5 | MAJOR | Draft 03 leaves cleanup-script failure behavior incomplete (`--skip-bd`, per-branch re-check) | **Applied.** Verified `--skip-bd` (`:13-17,:52`) and the per-branch `bd show` re-check (`:157-183,:244-273`). 03 specifies: pull fails → dry-run marks unverified, `--execute` deletes nothing and exits non-zero; bd absent → same; `--skip-bd` dry-run only; per-branch re-check kept; red-then-green tests in `scripts/test_cleanup_merged_remote_branches.sh` including "claimed after bulk list is not deleted". | 03 |
| 6 | MAJOR | Draft 03's command validation would reject AGENTS.md's deliberate negative examples | **Applied.** Verified AGENTS.md:7/:19 name `bd claim`, `bd assign --self`, `bd start`. Lint now checks only command lines in fenced shell blocks / command-leading inline code, against a checked-in subcommand list from `bd help` (so CI needs no bd), with an explicit forbidden-example marker; tests cover `bd sync` fixture fails and the negative examples pass. (Also resolves degraded minor 9.) | 03 |
| 7 | MAJOR | Draft 08's close of str-qwua7.12 leaves the multi-target exit-1 requirement unresolved | **Applied.** `bd show str-qwua7.12` confirms the acceptance check "exit 1 if at least one target succeeded and at least one failed"; `explore.rs:7075` asserts exit 0. Removed from the sweep's close list; new note 16 re-scopes it (done: single-target classes, typed `error_exit_code` at `main.rs:1475`; open: partial-failure policy decision, integration tests, SPEC.md:634 `--failure-threshold`). Sweep AC asserts .12 stays open. | 08, 16 |
| 8 | MAJOR | Draft 08 mistakes a newly created branch for landed work | **Applied.** Patrol check split into 17 and redefined on affirmative evidence: a `Merge branch '<id>-...'` commit on origin/main whose second parent adds commits; branch existence not required. Fixture tests for new empty branch, unmerged/uncommitted work, merged+deleted branch, reopened-after-landing, closed. | 08, 17 |
| 9 | MAJOR | Drafts 08/09 introduce an undefined WARN severity | **Applied.** Verified statuses are PASS/FAIL/PENDING/SKIP and the existing 14-day stale-claim FAIL. No WARN anywhere: the redundant "stale with no branch" WARN is dropped (existing FAIL covers it and is unchanged); 09's checks moved to new 18 as FAIL checks that land green because 09 fixes current violations first (inversions added to 09's AC); integration runs (green on main, forced red on fixture, exit codes) required in 17 and 18. | 08, 09, 17, 18 |
| 10 | MAJOR | Draft 07 duplicates str-qwua7.22 / .44 with conflicting acceptance order | **Applied.** `bd show str-qwua7.22` confirms its AC files findings before land-work. Skill rewrite stays owned by .22; new note 14 replaces its first acceptance check with land-then-file order. INDEX "Audits" section and path choice left to .44 (confirmed in `bd show str-qwua7.44`); 07/13 land at `audits/` (also degraded minor 7). Report recovery (13) split from 2026-09-22 publication (07). | 07, 13, 14 |
| 11 | MAJOR | Draft 07 requires a deletion cause that may be unrecoverable | **Applied.** Moved to new 15, time-boxed (about one session), sources searched listed, "cause undetermined" allowed, explicitly non-blocking for recovery/landing. | 07, 15 |
| 12 | MAJOR | Draft 10 drops the 80% UI thresholds and assigns no owner | **Applied.** Verified kapow and pickpackit memory: "UI code may be 80%"; zolem flat 90%. Goals, eligible-set definitions, metrics, recipes and baselines are quoted inline in a table (also degraded minor 11); AC requires a maintainer-named assignee before children start, per-project child completion conditions, eligible-set commands not dependent on a scratchpad. | 10 |
| 13 | MINOR | Drafts 01/02 present inconsistent measurements as one root cause | **Applied.** Traced numbers: 1,773 is the live-DB count at audit time (`areas/prior-audit-regress.md:82`), not the import size; JSONL has 1,733 records. 6 min = direct unwrapped `bd -v hooks run` (D4), 2m59.7s = direct run in a linked worktree (agent-repo-07), 234.9-301.2 s = hook-wrapped land.py (300 s cap). Table in 12; 12 requires one recorded baseline (command, binary, cwd, timing) and a trace attributing the wait; 02's latency targets are conditional on that attribution. (Also degraded MAJOR 3 and 4.) | 01, 02, 04, 12, BUNDLE.md |

Codex findings: 13 total; 13 applied, 0 partially, 0 disputed.

## Degraded (secondary) review findings

| # | Sev | Finding | Action |
|---|---|---|---|
| 1 | MAJOR | 08 blocked_by a note slug resolves to str-qwua7.1 and blocks forever | Applied: edge removed; ordering stated in prose (08, BUNDLE.md). |
| 2 | MAJOR | 08 carries three jobs | Applied: patrol check split to 17; str-qwua7.62 decisions left on .62 (1fik/wfd2 row removed from the table). |
| 3 | MAJOR | 02's `< 15 s` target rests on an unproven cause | Applied via new 12 (diagnosis blocks 02; targets conditional). |
| 4 | MAJOR | Durations contradict; partial imports cut by the timeout | Applied (see Codex 13); 01 scan looks for partial imports. |
| 5 | MAJOR | 07 duplicates .22 and bundles recovery/forensics/landings/skill | Applied (see Codex 1, 10, 11). |
| 6 | MINOR | 98 vs 97 files | Applied: re-counted 97 (`git ls-tree -r --name-only e067979d -- audits/2026-09-04 \| wc -l`), in 13. |
| 7 | MINOR | Target path reads .44 backwards | Applied: `audits/` is the landing path (07, 13). |
| 8 | MINOR | "Stop new damage" step impractical | Applied: 01 says damage continues; 02 re-runs 01's scan up to the moment the import is disabled. |
| 9 | MINOR | Docs check needs bd in CI | Applied (see Codex 6). |
| 10 | MINOR | Inversion FAIL goes red on day one; 09 too large | Applied: 09 fixes inversions first; checks split to 18. |
| 11 | MINOR | 10's evidence lives outside the repo | Applied (see Codex 12). |
| 12 | MINOR | 11 adds little | Not applied: kept as a short comment on the closed str-qwua7.17 so the record shows the regression; it now points at the recurrence check 17. |

## Splits, conversions, new slugs

- 02 split: diagnosis moved to new **12 beads-hook-stall-diagnosis** (blocks 02).
- 07 split into **07 publish-audit-reports** (2026-09-22 report only, filing
  bootstrap), new **13 audit-2026-09-04-report-recovery**, new
  **14 qwua7-22-audit-land-before-file-note** (note-to-existing str-qwua7.22),
  new **15 audit-branch-deletion-cause**.
- 08 split: patrol check moved to new **17 landed-not-closed-patrol-check**;
  str-qwua7.12 close replaced by new **16 qwua7-12-rescope-note**
  (note-to-existing str-qwua7.12); .62 execution left on .62; blocked_by
  `qwua7-1-git-state-check` removed.
- 09 split: patrol checks moved to new **18 triage-drift-patrol-checks**.
- No slug removed or converted. Slugs unchanged: 01-11.

## Cross-bucket effects

- shatter-cli-flags-and-help `cli-minor-output-and-help-polish` item 11 still
  proposes closing str-qwua7.12; that conflicts with `qwua7-12-rescope-note`
  and should be changed to "comment only, do not close"
  (`help-tracker-ids-lint` references an `exit-codes-qwua7-12-note` slug that
  does not exist; `qwua7-12-rescope-note` can fill that role).
- `tracker-reconciliation-sweep` no longer has a blocked_by on
  `qwua7-1-git-state-check`; ordering is prose only.
- `publish-audit-reports` no longer rewrites `/audit` SKILL.md; that stays
  with str-qwua7.22 via `qwua7-22-audit-land-before-file-note`. Bucket
  shatter-agent-guidance-and-repo-hygiene `repo-skills-rot` references
  `publish-audit-reports` for the audit skill; it should point at str-qwua7.22.
- The filing bootstrap needs the filer to file only the epic plus
  `publish-audit-reports` first (possible today with `ONLY=shatter` plus a
  long `HOLD` list; an include-list option in `file-all.sh` would be simpler).
