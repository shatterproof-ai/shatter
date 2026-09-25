# Revision: bento-guards-doctor-tracker (2026-09-23)

Inputs: Codex review `issues/crosscheck/bento-guards-doctor-tracker.codex.md` (primary) and the same-runtime review `issues/crosscheck/bento-guards-doctor-tracker.md` (secondary). The Codex reviewer could not query the tracker. Every overlap claim was checked with `bd show` in /home/ketan/project/bento: l01v, i76i, 49pg, nljv, 8oj0, wzbt, x4bm, sy49, 79j2 and bo9c are open, m4y5 is in_progress, xy8m and a0nz are open, and 2p2p is **closed** (3731a6f; the secondary review said in_progress). Code facts were re-verified at bento origin/main 0b8d488. The guard bypass list was re-probed with synthetic payloads, and every listed bypass still exits 0.

## Codex findings

| # | Sev | Finding (one line) | Action | Files |
|---|---|---|---|---|
| 1 | BLOCKER | #09 deletion criteria don't establish that the contents are disposable | applied: converted #09 to a note on bento-nljv. nljv already keeps "never delete non-empty automatically" and uses the shared 8oj0 orphan rule. The note adds a `content_class` (empty / build-output-only / other) with sample paths, so each path can be reviewed and removed individually. `other` is never removed. | 09 |
| 2 | MAJOR | #03 warning is invisible: land.py `_run_script` captures child stderr and drops it on success | applied: the warning must be printed while the step is still running and reach land.py's stderr and progress log. There is an end-to-end land.py test that asserts ordering. Now blocked by land-py-invocation-progress-log (Popen/heartbeat plumbing). | 03 |
| 3 | MAJOR | #04 wrongly requires sync between linked worktrees | applied: `bd worktree --help` confirms linked worktrees share one DB. The guidance now covers only separate clones and machines. The missing-remote doctor warning was dropped because a worktree-only repo legitimately has no remote. | 04 |
| 4 | MAJOR | #05 has no workable session-ownership contract | applied: dropped session attribution. The primary branch in the primary checkout is always report-only. The land.py marker is per branch and records pid + pid start time + host. The draft specifies stale and reused-pid handling, concurrent landers and exclusive create, with a test per case. A bare background `git push` is explicitly not covered. | 05 |
| 5 | MAJOR | #06 moves shared writes onto an unlocked RMW with a fixed `.tmp` name | applied: flock'd writer and unique temp file. An 8-process concurrent test must show the pre-fix writer losing keys. Migration merge rules (copy, repo-wide wins with notice, seen-lists unioned). Key scope table. Handles bare-primary and true-bare repos. | 06 |
| 6 | MAJOR | #08 moves creation to TMPDIR but the doctor still scans only /tmp | applied: discovery uses the repo's registered `land-work-preview-*` worktrees wherever they are, plus a glob over `gettempdir()` and `/tmp` for unowned previews. A test shows a non-/tmp stale preview is reported. | 08 |
| 7 | MAJOR | #10/#11 overlap on an unspecified tracker-closing lifecycle | applied: bento-x4bm (after wzbt/sy49/79j2/bo9c) owns closing after a landing. #11 dropped its land.py/verify-landing criterion and is now a closure `stale_claims` report only. #10 covers only non-landing closes and is blocked by bento-wzbt. | 10, 11, 19 |
| 8 | MAJOR | #10/#12 leave direct `bd close` unprotected | applied: both drafts now limit the enforcement claim (the helper is the supported path; direct `bd close` bypasses it). Detection is through `close.py --audit`, and #12 extends the audit to closed parents with open children. #12 adds a companion comment asking x4bm to run the open-children check at preflight. | 10, 12 |
| 9 | MINOR | #10 reason rules contradict the accepted forms (`wontfix: obsolete` < 20 chars) | applied: validation is typed per form. Free prose is rejected at any length. `wontfix` needs a rationale of at least 3 words. Valid and invalid cases are listed for each type. | 10 |
| 10 | MAJOR | #14 assumes an executable report validator that doesn't exist | applied: the requirement is now a review-checklist item in `quality-standards.md` and SKILL.md, with a grep test. The note states that no validator or component-grade template exists. | 14 |
| 11 | MAJOR | #14's evidence alternatives still allow its motivating mistakes | applied: evidence is required by claim type. Wiring claims need production reachability (a probe is not enough). Behaviour claims need a probe in the default configuration (a caller citation is not enough). | 14 |
| 12 | MAJOR | #04 closing proof depends on shatter staying unmigrated | applied: the proof is a checked-in fixture that reproduces shatter's pre-migration hook and AGENTS.md. Real shatter output is optional. | 04 |
| - | verdict | Reconcile related tracker issues before filing | applied: see the secondary-review rows below and the new note drafts 16-19. | 01, 04, 09, 10, 11, 16-19 |

## Secondary (same-runtime) review findings

| Sev | Finding | Action | Files |
|---|---|---|---|
| BLOCKER | 01 duplicates bento-l01v (segmenter, false positives, rtk/command/env) and partly bento-i76i | applied: 01 rescoped to the residual bypasses (`/usr/bin/git`; `timeout`/`nice`/`ionice`/`sudo`; `-C`; earlier `cd`; `GIT_CONFIG_*` env). It must build on `shell_segments.py` (a grep test forbids a second tokenizer) and is blocked by l01v and i76i through notes 16 and 17. | 01, 02, 16, 17 |
| MAJOR | 01 adds `switch`/`update-ref` while calling policy changes out of scope | applied: both verbs moved to a note on bento-i76i, which owns primary-branch verbs | 01, 17 |
| MAJOR | 04 overlaps bento-49pg (one-sentence rule in both skills, tracked-export doctor check) | applied: removed the consistency edit; blocked by 49pg through note 18; kept only the section and two net-new checks | 04, 18 |
| MAJOR | 09 overlaps bento-nljv and ignores bento-8oj0 | applied (same as Codex #1) | 09 |
| MAJOR | 10/11 overlap the closure group | applied (same as Codex #7) | 10, 11, 19 |
| MAJOR | 13 targets templates that don't exist (audit subagent prompt, `proj/` examples) | applied: verified with `git grep` and retargeted to real parallel-dispatch templates: code-bloat-sniffer step 4 (with its Go scratch-dir instruction) and swarm's teammate hygiene rules | 13 |
| MAJOR | 05 reflog criterion can't be checked | applied (same as Codex #4) | 05 |
| MINOR | 03 test threshold, `core.hooksPath`, marker comparison | applied: threshold is a parameter/CLI flag (no env var, per D4); hooks come from `git rev-parse --git-path hooks`; warns on a (major, minor) comparison against `bd version`, with a 5 s timeout | 03 |
| MINOR | 04 `bd` subprocess at SessionStart | applied: the remote check was dropped; check (a) only reads files | 04 |
| MINOR | 06 coordination with m4y5/xy8m; which keys stay per checkout | applied | 06 |
| MINOR | 08 repo-scoped reporting orphans the leaked previews | applied: separate "orphaned previews" line, throttled to once a day | 08 |
| MINOR | 12 depends on 10's helper | applied: 12 now names both close paths; companion comment on x4bm | 12 |
| MINOR | Bundle SHA stale | applied: re-verified at 0b8d488; check-unpushed line numbers updated (515-540, 591-625, 695) | 05, BUNDLE |

Disputed: none.

## Split / convert notes

- **Converted:** `closure-orphan-worktree-dirs` (09) changed from `new` to `note-to-existing` on bento-nljv. The slug is unchanged, and no other bucket references it.
- **New note drafts** (each gives the filer a blocked-by edge to an existing issue):
  - `l01v-residual-bypasses-note` (16, on bento-l01v)
  - `i76i-switch-update-ref-note` (17, on bento-i76i)
  - `49pg-dolt-remote-section-note` (18, on bento-49pg)
  - `wzbt-manual-close-note` (19, on bento-wzbt)
- **Companion comment:** followups-as-siblings (12) carries a `## Comment for bento-x4bm` section.
- **Priority inversion to flag:** git-guard-bypasses-and-false-positives is P1 but is blocked by bento-l01v and bento-i76i, which are both P2. Note 16 suggests raising l01v.
- Draft 15 (cross-check-stop-hook-hijack) was not in the Codex review scope and is unchanged. `bento-cross-check-bug/BUNDLE.md` holds a copy of it but has no NN file, so only this bucket's 15 is filed.
