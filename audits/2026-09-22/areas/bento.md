# Area review: bento plugin as it affects agent behaviour in shatter

Audit 2026-09-22. Reviewer scope: first-party bento plugin
(`/home/ketan/project/bento`, installed as `~/.claude/plugins/cache/bento/bento/2.3.82`)
and how it shapes agent behaviour in the shatter repo. I only observed. Nothing was fixed,
filed or committed.

## Method

- Read the prior audit (2026-09-04 report §10 and bento items 44-52) and the bento
  tracker. Epic `bento-rdtn` (15 children, one per 2026-09-04 finding) is **all closed**.
  51 bento issues are open and 6 are in progress; the ones that matter here are
  bento-e583, bento-neng, bento-qiw, bento-by8, bento-jdg and bento-zr6w.
- For each closed rdtn fix, I checked whether it actually changed outcomes in shatter.
  The evidence comes from 113 shatter session transcripts modified since 2026-09-05. I
  parsed them with a scratch script into about 12.4k Bash command/result rows, and I also
  read the background-task outputs under `/tmp/claude-1000/-home-ketan-project-shatter/*/tasks/`.
- I ran bento scripts read-only: the agent-env-doctor check functions imported in-process,
  and the git guard fed synthetic PreToolUse payloads.
- Checked the live state: `/tmp/land-work-preview-*`, the worktrees under
  `~/.local/share/worktrees/shatter`, remote branches, beads hooks, and the verifier manifest.

## Headline

The 2026-09-04 round converted every bento finding into a closed issue. Several of those
fixes were **tested in isolation but do not work in shatter's real conditions**: many
concurrent sessions, one shared primary checkout, slow beads hooks and linked-worktree-only
work. Specifically:

- The stale-preview refusal (rdtn.3) now **drives concurrent landings to destroy each
  other's previews**. On 2026-09-20 two sessions deleted each other's previews in a loop
  for about 10 minutes.
- land.py's `finally` cleanup (rdtn.14) **deletes the verifier log that rdtn.4 was
  created to keep**. The failure JSON then points at a file that no longer exists.
- The doctor's "seen once" state (rdtn.2) lives in an untracked per-checkout file. Every
  linked-worktree session, which is every shatter session, sees the full nudges again.
- The hook-bypass guard (rdtn.15) blocks `git commit --no-verify`. It does not block
  `/usr/bin/git`, `rtk git`, `cd <primary> &&` or `git -C` forms, and an agent pushed with
  `/usr/bin/git -c core.hooksPath=/dev/null` after the guard shipped.
- The all-cached verifier guard (rdtn.6) is inert for shatter. Its hand-written verifier
  predates the `executed` field, and nothing prompts a migration.
- Doctor detection (rdtn.1) works, but nothing acts on it. The five orphan worktree
  directories named in the 2026-09-04 audit are still there (682 MB).

Separately, **the beads git hooks cost about 300 s on every worktree creation, preview
creation and merge**. Bento's own scripts pay this cost silently, bento's guard forbids the
workaround agents had learned, and the guard's error message points to a reference that
says nothing about hooks.

## rdtn fix efficacy table

| rdtn | Fix | Works in shatter? | Evidence |
|---|---|---|---|
| .1 doctor detects bare/prunable/stale/orphan | yes, detects | detection only; nothing removes | 5 orphan dirs from Jun-Jul still present (F10) |
| .2 dormancy decision path, collapse repeats | code ok | **defeated in linked worktrees** | F8 |
| .3 refuse new preview while leftovers exist | code ok | **causes cross-session preview destruction** | F1 |
| .4 persist verifier log, killed vs failed | code ok | **log deleted by land.py cleanup** | F2 |
| .5 land from preview when primary diverged | yes | used by land.py | - |
| .6 require `executed`, fail on all-cached | code ok | **inert: shatter verifier emits no `executed`** | F13 |
| .7 bootstrap `--claim` | yes | bootstrap-b/c/d logs show claim | - |
| .10 superpowers precedence | yes | doctor prints it once per repo (but see F8) | - |
| .13 bootstrap bare checkout | yes (core.bare is now false) | - | - |
| .14 land.py single driver | yes, used in about 15 of 35 landings | but see F2, F5, F6, F7, F18 | - |
| .15 git guard for primary mutation and hook bypass | partial | **trivially bypassed; bypass observed** | F4 |

## Findings

### F1 [P1, L4/AGENT] Leftover-preview refusal plus its hint makes concurrent landers delete each other's live previews

`land-work-create-preview.py:62-81` treats every registered `land-work-preview-*` worktree
as a leftover. It has no owner, PID, lock or age check. Its error text (`:269-271`) tells
the agent to `remove them first (land-work-create-preview.py --cleanup ...)`. land.py never
passes `--allow-existing`, so two concurrent land.py runs in one repo cannot coexist.

Observed ping-pong on 2026-09-20 between sessions `87606e10` and `9f13ca23`:

- 23:49: `87606e10` land.py `create_preview: failed (0.054s)` with "leftover
  land-work-preview-* worktree(s) exist: /tmp/land-work-preview-mz7t9g27". The agent
  inspected it (`git status`: "All conflicts fixed but you are still merging", which is a
  live preview) and ran `land-work-create-preview.py --cleanup --preview-dir
  /tmp/land-work-preview-mz7t9g27`.
- 23:49: `9f13ca23`, whose preview that was, got `verify: failed (238.362s) ... cleanup:
  failed`. At 23:50 it `rm -rf`'d `/tmp/land-work-preview-mz7t9g27`.
- 23:54-23:55: the reverse. `9f13ca23`'s `d1bfx2yj` vanished, and `87606e10` ran `--cleanup`,
  then `git worktree remove --force` and `rm -rf` on `g663hhnu`, while `9f13ca23` got
  `verify: failed (19.927s)`.
- 00:00: `87606e10` `create_preview: failed (300.528s)` with a traceback: `FileNotFoundError:
  ... '/tmp/land-work-preview-2vu7job9'`. Its own preview was deleted while `git worktree
  add` was still running.

bento-e583 (P1, open) reports the same symptom from kapow and names the mechanism only as
"suspected". This shatter evidence pins it down: the rdtn.3 guard plus its remediation hint
is the trigger.

**Recommendation:** write an owner lockfile (PID, session, start time) into each preview at
creation, and hold an `flock` for the whole driver run. The leftover check should skip
previews whose lock is held or whose PID is alive, and should say "another landing is in
progress; wait or retry" rather than "remove them". `--cleanup` should refuse a preview it
does not own unless `--force-foreign` is passed. Add a two-process test.

### F2 [P1, L1] land.py deletes the verifier log it reports as `output_path`

land.py calls `land-work-run-verifier.py` without `--log` (`land.py:296-305`), so the log
defaults to `<preview>/.land-work/verifier.log`. On failure, `StepFailure` carries
`output_path=payload["verifier_log"]` (`:127`). The `except StepFailure` path then calls
`cleanup_preview()` (`:366`), which removes the preview and the log with it.

All three `verify: failed` results in `9f13ca23` (09-19 15:38, 09-20 23:49, 09-20 23:55)
report `"output_path": "/tmp/land-work-preview-XXXX/.land-work/verifier.log"` after
`cleanup: passed`. On 09-19 the agent then had to recreate a preview by hand
(`e53yb8el`) and rerun `task test-standard` to see the failure. That manual preview was
later reported as a "leftover" (09-19 22:33).

This undoes rdtn.4, and shatter memory
`project_land_work_verifier_first_run_flake.md` ("retry, it very likely passes") remains the
de-facto procedure.

**Recommendation:** default land.py's `--log` to a path outside the preview, for example
`$XDG_STATE_HOME/bento/land-work/<repo>/<branch>-<ts>.log`. Include the tail in the failure
JSON. Add a test asserting that `output_path` exists after a failed run.

### F3 [P1, AGENT/L5] The beads hook cost (about 300 s per checkout-type operation) is paid silently by bento scripts, and bento forbids the workaround

The shatter `.git/hooks/post-checkout`, `post-merge` and `pre-push` are beads v0.63.3 blocks
with `_bd_timeout=${BEADS_HOOK_TIMEOUT:-300}`. The installed `bd` is 1.1.0, so the hook
template version is out of date. `git worktree add` fires post-checkout, and the land.py
step timings show that cost:

- `create_preview: passed (234.9 / 235.1 / 239.7 / 239.9 / 253.6 / 268.9 / 297.8 / 300.5 /
  300.8 / 301.0 / 301.2 s)` across 11 successful landings.
- The verify step itself took 266-578 s.

Every `launch-work-bootstrap.py --apply` in these sessions exceeded the Bash foreground
timeout. Examples: 09-19 14:30 (>120 s), 09-21 22:09 (>600 s), and this audit's own launch
(09-22 17:02, >120 s). The bootstrap captures the hook stderr, so agents see an unexplained
hang. Background outputs contain `beads: hook 'post-checkout' timed out after 300s`
8 times, including in this audit session (`63ab6471/tasks/bevju0yf6.output`). Several
sessions (ad59d49b, 87606e10 twice, 5f2377ef) read `.git/hooks/post-checkout` trying to
work out the hang, and one retried with `BEADS_HOOK_TIMEOUT=10`.

At the same time, `require-worktree-git-guard.py` blocks `--no-verify` and
`-c core.hooksPath` (observed blocks at 09-21 15:55 and 09-22 16:20). Its message says "see
the launch-work skill's dependency-bootstrap guidance for slow-hook fixes", but
`launch-work/references/dependency-bootstrap.md` has **no content about hooks at all**.
Shatter's MEMORY.md still instructs `core.hooksPath=/dev/null` and `--no-verify`.

**Recommendation (bento):**

1. Preview worktrees are throwaway, so create them with beads hydration disabled for that
   one `worktree add`, and document the reasoning. `BD_GIT_HOOK`/`BEADS_HOOK_TIMEOUT=5` in
   the subprocess env is the least invasive option.
2. Have bootstrap and create-preview time the git subprocess, and emit a warning naming the
   slow hook when it takes more than 30 s.
3. Add a doctor check comparing the beads hook marker version with `bd version`, and
   measuring `bd hooks run post-checkout` latency.
4. Write the missing "slow hooks" section that the guard message cites.

Shatter-side counterpart: str-qwua7.28 (open 18 days) and str-mpgg1 (see F17). They
contradict each other.

### F4 [P2, L1] The git guard (rdtn.15) is bypassed by the command forms agents actually use here

I fed synthetic payloads to `require-worktree-git-guard.py` (exit 2 means blocked):

```
exit=2 cwd=shatter  git merge feature
exit=0 cwd=worktree git -C /home/ketan/project/shatter merge feature
exit=0 cwd=worktree cd /home/ketan/project/shatter && git merge feature
exit=0 cwd=shatter  /usr/bin/git merge feature
exit=0 cwd=worktree /usr/bin/git commit --no-verify -m x
exit=2 cwd=worktree git commit --no-verify -m x
exit=0 cwd=worktree rtk git commit --no-verify -m x
exit=0 cwd=worktree env GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath ... git commit
exit=0 cwd=shatter  git update-ref refs/heads/main HEAD~1
exit=0 cwd=shatter  git switch main
```

`_find_git_segments` only matches a bare `git` token, and the primary-checkout test uses the
hook cwd rather than the effective `-C`/`cd` target. Shatter memory recommends `/usr/bin/git`
(rtk workaround), and the dotfiles RTK rule mandates an `rtk` prefix. So the "common,
unobfuscated case" the docstring targets is exactly the one that goes unchecked.

A real bypass after the guard shipped: `c1689435` at 2026-09-22 00:48 ran
`/usr/bin/git -c core.hooksPath=/dev/null push -u origin str-hjrnp.3`. Also: `e1d1c770`
`/usr/bin/git push --no-verify` (09-08), and `e724dbd8` pushed to `refs/heads/main` with
`--no-verify --force-with-lease` four times on 09-07/08.

**Recommendation:**

- Match `(^|/)git$` and `rtk git`.
- Track `cd <dir>` within the segment chain and `-C <dir>` to compute the effective repo.
- Add `switch` and `update-ref refs/heads/<primary>`.
- Check `GIT_CONFIG_*` env assignments.
- Add a table-driven test with these ten cases.

### F5 [P2, L4] land.py aborts or resets merge state in the shared primary checkout that it may not own

`abort_primary_merge_if_in_progress()` (`land.py:144-148`) runs `git merge --abort` whenever
`<primary>/.git/MERGE_HEAD` exists. It is called on every StepFailure (`:366`), on every
BaseException (`:378`) and on SIGINT/SIGTERM (`:350`), including failures at `prepare`,
`create_preview` or `verify`, before this driver has touched the primary. The primary
checkout is shared by concurrent sessions (memories `feedback_swarm_shared_checkout_landing_race`
and `feedback_primary_checkout_diverged_local_main`). The tree-mismatch branch also runs
`git reset --hard HEAD@{1}` in the shared primary (`:201`).

I did not observe a collision caused by this. Confidence is medium; it is a code-read risk.

**Recommendation:** set `self.merge_started = True` immediately before the primary
`git merge`, and only abort or reset when it is set. Record the pre-merge SHA and reset to
that SHA rather than `HEAD@{1}`.

### F6 [P2, L6/AGENT] land.py runs 10-25 minutes, but the skill gives no invocation guidance, and agents mask its exit status

The skill says only `land-work/scripts/land.py --runtime <runtime>` (SKILL.md:262). A
typical run is create_preview (about 300 s) + verify (266-578 s) + merge_push (259-1108 s).

In `9f13ca23` agents invoked `land.py --runtime claude 2>&1 | tail -100` 14 times. Four
failed runs (`prepare: failed`, `create_preview: failed`, `verify: failed`) returned
`[exited with code 0]` because `tail` masked the exit status. This is the exact pattern
shatter memory `feedback_never_pipe_git_commit_through_tail.md` warns about. Other sessions
reinvented `> log 2>&1; echo exit=$?` and background-polling loops.

**Recommendation:** add a canonical invocation block to land-work: `run_in_background`, stdout
to a JSON file, stderr progress to a log, then read the exit code. Have land.py write its
own progress log and print the path first, and emit a heartbeat line every 60 s.

### F7 [P2, L4] `prepare --require-up-to-date` forces a rebase, force-push and hook round before every landing, although the preview already verifies the exact merge

land.py hard-codes `--require-up-to-date` (`land.py:274`). Observed:

- `prepare: failed ... current branch is behind the primary branch by 2 commit(s)`
  (87606e10 09-20 04:53)
- `... by 4 commit(s)` (9f13ca23 09-21 15:19)
- `... has no commits ahead ...; behind by 1` (09-20 23:23)

Each rebase fires post-checkout (F3), requires a force-push (which runs the pre-push hook's
`task affected`), and inflates the Stop hook's unpushed count (F11). The preview step merges
feature into the leased SHA and verifies that exact tree, so a stale base is already covered
by exact-candidate verification.

**Recommendation:** drop `--require-up-to-date` from land.py, or make it advisory. Keep the
conflict path (preview merge conflict leads to "rebase needed"). If rebase-before-land is
repo policy, make it a verifier.json or swarm-config option.

### F8 [P2, AGENT] Doctor "seen/decided" state is per-checkout, so every linked-worktree session gets the full nudges

`_seen_plugins`, `_skipped_plugins` and `_superpowers_pointer_seen` read `<root>/.agent-mode.local`,
where root is the session's git toplevel. That file is untracked and exists only in the
primary checkout. I evaluated the doctor in-process:

- audit worktree: `seen= frozenset()`. It prints both full dormancy paragraphs and the
  superpowers notice ("This notice prints once per repo").
- primary: `seen= {'bugshot','storystore'}`, with one-liners.

Shatter forbids work in the primary, so every working session gets the uncollapsed text.
The doctor also writes a new `.agent-mode.local` into each worktree root (`:1085`), which
then counts as untracked residue for land-work step 9b.

**Recommendation:** resolve `.agent-mode.local` via `git rev-parse --git-common-dir` (the
primary's), for both reads and writes. The same fix applies to `require-worktree-git-guard.py`
`_read_agent_mode_keys` (`hook_bypass=allow` set in the primary is ignored in worktrees).

### F9 [P2, L2/AGENT] The stale-preview doctor check is global across repos, has one line per directory, and bento's own test suite floods it

`check_stale_previews` globs `/tmp/land-work-preview-*` without checking which repo owns
each one, and emits one warning per entry. `/tmp` currently holds 89 such directories:

- 86 were created today and 3 on 09-21.
- 87 have a `.git` pointing at `/tmp/tmpXXXX/repo/.git/worktrees/...`, whose parent temp
  repos are deleted.
- Their contents are `README.md` and `feature.txt`, which are bento test fixtures.

`default_preview_dir()` hard-codes `dir="/tmp"` (`land-work-create-preview.py:85`), so tests
cannot redirect it via `TMPDIR`. Simulating now + 2 days with `collect_warnings` gives **98
warnings** for the shatter worktree, 89 of them stale-preview lines, all injected into
SessionStart context in every repo.

The warning also says "remove it or let closure clean it up", but `closure-scan.py`'s
`--apply` choices are only `delete-local-merged-branches` and
`delete-local-patch-equivalent-branches` (`:1659-1660`). Closure has no preview or orphan
handling.

**Recommendation:**

- Honour `TMPDIR` in `default_preview_dir`, and have the test suite set `TMPDIR` to the
  pytest tmp dir.
- Scope the doctor check to previews whose gitdir belongs to this repo, and collapse the
  output to one line with a count.
- Add a `closure --apply remove-stale-previews` option, or remove the "let closure" wording.

### F10 [P2, AGENT] Detection without remediation: orphan worktree directories flagged since 09-04 still exist

The doctor now reports:

```
orphan worktree directory: .../str-6q1i ... safe to remove   (2026-06-18, 109M)
.../str-hszo-tmpfix (2026-06-17, 573M)
.../str-k6e61-scm-followups, str-mambd-enum-variant-gen, str-yhsp-concolic-run (Jul, 16K each)
```

These are the same five directories listed in 2026-09-04 agent-system.md:39. The doctor
says "safe to remove", but shatter AGENTS.md:513 says "**Never** run `git worktree remove`
or delete the worktree directory yourself". That rule is the prior-audit contradiction #7,
still present. No bento skill has an apply path for these, so every session reads the
warning and nobody acts.

**Recommendation:** add `closure --apply remove-orphan-worktree-dirs`, with a dry run by
default, a size report and a liveness check. The doctor should point at that command. The
shatter AGENTS.md line needs reconciling (str-qwua7.23).

### F11 [P2, L4] check-unpushed overcounts after rebases and blames the session for peer commits on shared `main`

`ahead_count` is `git rev-list @{u}..HEAD --count` (`check-unpushed.py:398`). After a rebase
onto main, every main commit that is not in the branch's old upstream counts as unpushed:
"branch 'str-6vl7p-redundant-canonicalize' has 71 unpushed commits" (10x), "69" (10x),
"str-rmcrl... 72" (6x).

In the shared primary it blocks on peer-session commits: "branch 'main' has 2 unpushed
commits" (17x) and "main has uncommitted changes and 1 unpushed commit" (13x). The hook
says it can block mid-session, which pushes agents toward a premature force-push.

**Recommendation:** count `git rev-list HEAD --not --remotes`, meaning commits that exist on
no remote. For the primary branch in the primary checkout, report without blocking unless
this session created the commits (compare against the reflog since session start). Related:
bento-neng (re-nag), in progress.

### F12 [P2, L4/AGENT] The swarm skill tells the lead to land from inside the teammate's worktree, contradicting a shatter lesson

swarm SKILL.md "Serial-Mode Landing" step 3: "Invoke `bento:land-work` from within the
teammate's worktree". land-work requires rebasing there (F7). Shatter memory
`feedback_lead_landing_prep_separate_worktree.md` (09-08) records that doing exactly this
moved teammates' HEADs while they were doing review fixes: "the lead's `git checkout` had
silently moved the worktree's HEAD out from under them". bento-qiw fixed who lands, not
where.

**Recommendation:** swarm should have the lead land from a lead-owned scratch worktree
checked out at the teammate's branch tip, or confirm the teammate session has exited before
touching its worktree. land.py could accept `--branch <name>` so it can run from any
worktree.

### F13 [P2, AGENT] The rdtn.6 all-cached guard is inert for existing manifests, and shatter's verifier discards all output

`land-work-run-verifier.py:525-541` trips only when every check reports `executed: false`.
Payloads with no `executed` field pass. Shatter's `scripts/land_work_verifier.sh` (hand-written
2026-08-05, d01a22db) emits `{"name":..,"status":..}` only, and runs each gate with
`>/dev/null 2>&1`. So land.py never prints `[cached]`/`[executed]`: all 11 observed verify
lines lack it. Even a surviving `verifier.log` (F2) would contain just the JSON line.

`wire-land-verifier.py:853-917` now generates `executed`, but nothing prompts existing repos
to regenerate. Shatter-side: str-qwua7.55 and str-qwua7.2 (open).

**Recommendation:** run-verifier should warn, visibly and inside land.py's step line, when a
manifest's checks lack `executed`. The doctor should flag manifests whose wrapper predates
the current contract. The contract should require that check stdout/stderr reach the
verifier's own stdout.

### F14 [P2, L3] land-work SKILL.md is 44.6 KB / 6,376 words, contradicts itself, and is bypassed

- The skill is still written around the manual flow. Steps 8 and 8i-8iv, batch landing
  (`:545-722`) and the anti-rationalisation table all load on every landing, although land.py
  now covers the serial path.
- Its Command Rule (`:105-106`) forbids `$(...)`, but the skill's own commands use it at
  `:143`, `:181-182`, `:449` and `:670`.
- `bento:land-work` was invoked 9 times across the 35 first-parent merges to shatter main
  since 09-05. Other landings were hand-driven by scripts, including 4 raw
  `git push --no-verify origin <sha>:refs/heads/main --force-with-lease` by `e724dbd8`.
- Its Tracker Handoff says `.beads/issues.jsonl` "may be intentionally untracked... do not
  re-add or commit it during landing". In shatter the file is tracked, and AGENTS.md
  mandates committing it (F16).

prior_ref bento-by8 (trim largest skills).

**Recommendation:** make the serial skill a short "run land.py, read its JSON, fix the named
step" page. Move the manual flow, batch mode and manifest rules into references. Resolve the
`$(...)` contradiction by moving those computations into scripts.

### F15 [P2, L4] Landing never deletes the remote feature branch

land-work step 10 deletes only the local branch and worktree, and land.py does no remote
cleanup. `git branch -r --merged origin/main | wc -l` gives **39** merged remote branches on
shatter's origin (67 remote branches total). Shatter AGENTS.md compensates with a manual
post-swarm step (`git push origin --delete`, or `scripts/cleanup-merged-remote-branches.sh`).

**Recommendation:** have land.py delete `origin/<feature>` after verify-landing when the
branch is fully merged, with a repo opt-out in verifier.json or swarm-config. Add a closure
mode for merged remote branches.

### F16 [P2, AGENT] Beads snapshot and sync procedure is undefined, so the tracked snapshot is 15 days stale and there is no remote

- Shatter AGENTS.md:363-368 mandates `bd sync` once at landing, but `bd sync` gives
  `Error: unknown command "sync" for "bd"` (bd 1.1.0).
- bento land-work says do not commit the jsonl, and beads-issue-flow says nothing about the
  export snapshot.
- The last `.beads/issues.jsonl` commit is 134dd616 (2026-09-07). Tracked snapshot: 1,733
  issues. `bd count`: 1,773.
- Hook output repeats "beads: post-checkout JSONL import warning: no Dolt remote configured"
  (26x) with a repair hint. `bd dolt remote list` itself hung with `i/o timeout` during this
  audit.
- A hand-rolled "bd sync" commit was pushed straight to main from `/tmp/shatter-bd-sync-main`
  with `--no-verify` (19cbf3b5, 09-06).

Tracker state for three weeks exists only in one local Dolt DB.

**Recommendation:** give beads-issue-flow a "snapshot and remote" section: when the jsonl
is tracked, say who exports and commits it and when; when there is no Dolt remote, say how
to configure one. Add a doctor check for a tracked jsonl older than N days, or no Dolt
remote. Shatter must replace `bd sync` in AGENTS.md.

### F17 [P3, AGENT] No detector for claimed issues with no branch or worktree

`str-mpgg1` (P1, "Revert unauthorized beads-hook edits") has been `in_progress` since
2026-09-01 with no local branch or worktree. Its goal contradicts str-qwua7.28 (install
`BEADS_HOOK_TIMEOUT`). rdtn.9 made closure report issue-named branches whose issue is not
claimed. The inverse, a claim with no branch, is undetected. The claims show
`Owner: Test · Assignee: Test` (bd identity; str-qwua7.51).

**Recommendation:** add a closure or doctor check for `in_progress` issues older than N days
with no matching branch, worktree or remote branch. Report it; do not auto-release.

### F18 [P3, L6] land.py `merge_push` is an opaque 4-18 minute step that re-runs gates via the repo pre-push hook

Observed `merge_push: passed (258.6 / 259.8 / 298.7 / 309.2 / 498.6 / 594.4 / 614.9 / 678.8 /
1107.8 s)`. Shatter's pre-push hook runs beads, then `task check` for `refs/heads/main`. That
is the full gate, immediately after the verifier already ran test-standard, parity and
conformance on the same tree. land.py captures the hook output and records only the total.

**Recommendation:** split merge_push into `commit`, `push` and `sync` sub-records with
durations. Write hook stderr to the landing log. Let verifier.json declare
`push_hook_runs_gates: true` so land.py can say why a push takes 10 minutes. Whether to keep
double gating is a shatter decision (str-qwua7.55).

## Positives worth keeping

- land.py is a real improvement. When it works, its per-step lines
  (`prepare: passed (0.072s)` ... `verify_landing: passed (0.044s)`) and final JSON give
  agents a clear failure point. It was used for about 15 clean landings in 4 days, with gate
  evidence quoted in `bd close` reasons.
- The push-from-preview route (rdtn.5) removed the diverged-primary hazard that the
  2026-08-27 memory documented.
- `launch-work-bootstrap --claim` works, and closes the claim-at-launch gap.
- The git guard's message design (names the rule, the reason and the opt-out) is good.
  Coverage is the problem, not the UX.
- The doctor is always-exit-0 with bounded reads, and its checks are individually unit
  tested.
- The bento tracker is well kept: each rdtn child names its evidence and files.

## Agent-system root causes (cross-cutting)

1. **Fixes are validated in single-session fixtures, never under shatter's real conditions**
   (N concurrent sessions, a shared primary checkout, linked-worktree-only sessions, slow
   repo hooks). The rdtn.2, .3, .4 and .14 regressions all come from this.
   Recommendation: a bento "consumer-conditions" integration test harness (two concurrent
   land.py runs, a session in a linked worktree, a repo with a 30 s post-checkout hook), plus
   a post-release field check in one real consumer repo before an rdtn-style epic closes.
2. **Closure criteria are "code merged", not "symptom gone"**. rdtn closed 15/15 while the
   memory files it was meant to retire (`project_land_work_verifier_first_run_flake`,
   `project_beads_git_hook_timeout`, bypass advice) are still active in shatter's
   MEMORY.md.
3. **Detection without remediation.** Doctor warnings with no apply path become permanent
   noise (F9, F10).
4. **Guidance crosses repos without contract checks.** The guard message cites a reference
   with no such content (F3). The swarm skill contradicts a consumer lesson (F12). land-work
   contradicts shatter's beads sync rule (F14, F16).
