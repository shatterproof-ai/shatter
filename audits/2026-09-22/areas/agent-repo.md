# Agent system inside the shatter repo — audit 2026-09-22

Area: `agent-repo`. Levels AGENT and L3, with L2 where docs and practice
disagree. Observation only. All commands ran in the audit worktree
`~/.local/share/worktrees/shatter/audit-2026-09-22` (HEAD 16794cef == origin/main)
unless a path says otherwise.

## 0. Headline: why 64 of 77 prior-audit issues are still open

In the 18 days since the 2026-09-04 audit, 102 commits landed on main. The
split is lopsided:

| Where the prior audit's fixes belonged | Filed | Closed by 2026-09-22 |
|---|---|---|
| bento (`bento-rdtn`, 15 children) | 15 | **15** (all closed by about 09-08) |
| dotfiles GitHub #9–#13, #15 | 6 | **6** (closed 09-07 and 09-08) |
| shatter `str-qwua7` (77 incl. sub-epics) | 77 | **12** |
| of which shatter agent-system items (.22–.28, .51, .54, .55) | 10 | **1** (.27, delete `.claude/agents`) |

Evidence for the shatter tree: `bd list --parent str-qwua7 --all` reports
"Total: 77 issues (64 open, 0 in progress)". Of the open children, 47 have not
been updated since they were filed (`updated_at` 09-05: 37, 09-07: 10). 52 have
no assignee. None is in progress.

Causes, in order of weight. Each has its own finding below.

1. **The only scheduled forcing function has never run.** The Drift Patrol
   workflow has failed on all 7 scheduled runs since it was created
   (08-10 → 09-21). Its setup-go step points at a root `go.mod` that does not
   exist, so the job dies before the patrol starts. Nobody noticed, and no issue
   exists (AR-02).
2. **The report was never published.** Branch `audit-2026-09-04` is 102 commits
   behind main and 7 ahead, was never pushed (`git ls-remote --heads origin
   audit-2026-09-04` returns nothing), and was never merged. All 62 child issue
   bodies checked cite `audits/2026-09-04/...` files, which exist only on one
   local branch. `audits/` on main still holds only the 02-28 and 05-21
   reports (AR-04).
3. **Priority inflation buried the agent-system items.** There are 177 open
   issues: 45 P1, 111 P2 and 21 P3. 154 are ready. All the agent-system items
   were filed P2, so they never reach the top of `bd ready`. Meanwhile a new
   feature epic (`str-hjrnp`, Jev frontier ranking: 12 commits, 4 children) was
   planned, built and closed on 09-21/22 (AR-18).
4. **Fixed work does not close its issue, and closed work carries no record.**
   `.1`, `.18` and `.19` are substantially resolved but still open. `str-mpgg1`
   landed on 09-02 but is still in_progress. Seven closures since 09-14 have the
   reason "Closed" (AR-05).
5. **Decisions conflict and nothing resolves them.** `.28` (install a
   BEADS_HOOK_TIMEOUT section) rests on a code fact that the `str-mpgg1` revert
   deliberately removed, citing AGENTS.md's "leave the hooks alone". The tracker
   has no "needs-decision" state, so both stay open (AR-07).
6. **Landing is expensive, so small doc fixes do not get done.** The beads
   post-checkout hook took **2m59s** in a linked worktree today. Verifier runs
   in close reasons take 267–352 s. A docs-only AGENTS.md fix therefore costs
   more than 10 minutes of landing (AR-07, AR-11).
7. **Asymmetric contracts across repos.** bento shipped the fixes as *opt-in*
   capabilities: a verifier `executed` flag, a timeout, log capture, and doctor
   decision keys. Shatter never adopted them, and no shatter issue exists for
   the adoption itself. So "bento fixed it" did not change shatter's behaviour
   (AR-11, AR-15).

## 1. Findings

IDs match the structured output. P = priority, L = level.

### AR-01 (P1, AGENT/L1): the repository git identity is poisoned; every commit since June is authored "Test <test@example.com>"

- `/home/ketan/project/shatter/.git/config` contains `[user] name = Test`,
  `email = test@example.com`. `git config --show-origin --get-all user.name`
  shows the global `Ketan Gangatirkar` being overridden by that local file.
- `git log -300 --format='%an <%ae>' | sort | uniq -c` gives 115 × `Test
  <test@example.com>` and 185 × `Test User <test@example.com>`. The last real
  author commit is dated 2026-06-14. The first `Test User` commit is 131ebe06
  on 2026-06-23. All of these are pushed to github.com/shatterproof-ai/shatter.
- Source: fixture scripts run `git config user.name "Test"` / `"Test User"`
  (`scripts/test_cleanup_merged_remote_branches.sh:48,137`,
  `scripts/test_git_sandbox_test_lib.sh:32,68`,
  `scripts/test_target_dir_report_json.sh:28`,
  `scripts/test_git_sandbox_test_lib.py:94,135`). When one of them runs from a
  git hook, `GIT_DIR` points at the real repo. `str-jttrf` (closed 09-12) fixed
  that leak with reproduce-first tests, and `core.bare=true` from the same leak
  is now `false`. The poisoned identity was never repaired, and no check looks
  at it. `bd` "Owner: Test" on every issue has the same cause.
- The prior audit missed this. It showed up only as "Test User" in an assignee
  table.

### AR-02 (P1, AGENT): the Drift Patrol CI job has never run on schedule

- `gh run list --workflow drift-patrol.yml` shows 7 of 7 scheduled runs as
  `failure` (08-10, 08-17, 08-24, 08-31, 09-07, 09-14, 09-21), each taking
  17–29 s. The only success is the PR self-test.
- `gh run view 35620815498` fails with "X The specified go version file at:
  go.mod does not exist". `.github/workflows/drift-patrol.yml:85` has
  `go-version-file: go.mod`. `ci.yml:53`, `perf-ci.yml:36` and
  `release.yml:131` all correctly use `shatter-go/go.mod`. The July audit memory
  had already recorded this exact bug for ci.yml.
- `docs/DRIFT-PATROL.md:27-28` says the self-test on PRs means "the patrol
  cannot rot in place". The self-test only unit-tests the Python, so it cannot
  see this failure. `scripts/test_ci_workflow_structure.py` checks ci.yml only
  and never mentions drift-patrol.yml.
- The "Owner" row in `DRIFT-PATROL.md:24` names "the maintainer on the weekly
  triage rotation". No rotation exists.
- Run locally now, the patrol reports: tracker-hygiene FAIL (str-8q1b4 at 22 d
  and str-mpgg1 at 20 d stale in_progress; 4 open children under closed
  parents), plus two PENDING (`str-wurp`, `str-u394l.3`).
- `bd search "drift patrol"` and `bd search go.mod` return no issue.

### AR-03 (P1, AGENT): the Build and Release workflow has not passed in 200 runs, with no issue

- `gh run list --workflow release.yml -L 200` finds no `success` at all. The
  last 30 are all `failure`, one per push to main.
- Run 35756993200: `x86_64-pc-windows-msvc` fails with "wrapper.h:1:10: fatal
  error: 'z3.h' file not found". `aarch64-unknown-linux-gnu` fails during the
  cross build. `ci.yml` stays green, so landings look healthy.
- `bd search windows`, `bd search aarch64` and `bd search "release workflow"`
  return nothing.
- Agent-system cause: nothing in land-work, pre-completion, drift-patrol or
  the doctor looks at post-push workflow status beyond ci.yml.

### AR-04 (P1, AGENT): audit reports are never published, so filed issues cite evidence nobody else can reach

- Details in §0 item 2. The same thing happened to the 2026-07-10 audit: its
  report was never committed, and str-sff87 was closed on 09-08 as "superseded
  … no 2026-07-10 report was committed".
- `.claude/skills/audit/SKILL.md` Post-Audit step 6 still says to commit on
  main ("Stage and commit … `audit: YYYY-MM-DD`"). The require-worktree hook
  blocks that, so the step can never be done as written. `str-qwua7.22` has
  been open since 09-05.

### AR-05 (P2, AGENT): tracker state lags reality in both directions

- Resolved but still open: `str-qwua7.1` (core.bare is `false` now), `.19`
  (`main == origin/main`, and `git worktree prune --dry-run` is empty) and
  `.18` (5 of its 6 parked branches landed 09-19 → 09-22 under their own ids:
  str-rmcrl, str-na9db, str-0z1im, str-6vl7p, str-vr7vq, plus str-duens).
  Only str-8q1b4 is still parked.
- Landed but in_progress: `str-mpgg1`, merged in 84941b37 on 09-02 and
  in_progress since 09-01.
- Closed with the bare reason "Closed" since 09-14: str-qwua7.8, .9, .9.2, .15,
  .56, str-leozr and str-ajjri. `str-qwua7.51` ("require a close reason") is
  open.
- By contrast, landings from 09-19 on through bento `land.py` carry rich close
  reasons with gate evidence (e.g. str-hjrnp.1: "verify: passed 352.78s; task
  affected exit 0 (gates: …)"). This is worth keeping.

### AR-06 (P1, L2/AGENT): AGENTS.md mandates `bd sync`, which bd 1.1.0 does not have; the tracked JSONL is 15 days stale

- `bd --version` gives 1.1.0. `bd sync --help` gives "Error: unknown command
  \"sync\"".
- AGENTS.md references `bd sync` at lines 125, 293, 340, 347–369 (the "Beads
  Sync Cadence" section), and `.claude/skills/audit/SKILL.md:385` does too.
  `.beads/PRIME.md` instead says "`bd dolt pull` → git commit". The two
  contradict each other.
- The last commit touching `.beads/issues.jsonl` is 134dd616 (09-07). Checked
  in that file, `str-qwua7.4/.7/.9/.15/.56` read `open` although all are closed
  in the DB, and the `str-hjrnp` epic is missing.
- Consequence: `scripts/cleanup-merged-remote-branches.sh` builds its
  "in-progress, protect" set from the tracked JSONL (AGENTS.md lines 270-273),
  so its protection logic now runs on data that is weeks old.
- `.beads/issues.recovered.jsonl` is tracked too, with no explanation.

### AR-07 (P2, AGENT): the beads hook timeout is deadlocked between two decisions, and agents keep bypassing hooks

- Measured today in the audit worktree: `time bd hooks run post-checkout …` →
  `real 2m59.667s`. The hooks still use `${BEADS_HOOK_TIMEOUT:-300}`
  (`.git/hooks/{pre-commit,pre-push,post-checkout,post-merge}:6`).
- `str-qwua7.28`'s "Current code facts" say `scripts/setup-hooks.sh:41` already
  defines the BEADS_HOOK_TIMEOUT section. It does not. `grep BEADS_HOOK_TIMEOUT
  scripts/ .beads/hooks/ Taskfile.yml` finds nothing, because b5cd25ec
  (`str-mpgg1`, 09-01) reverted it as an "unauthorized beads-hook edit" under
  AGENTS.md's "Leave the managed git hooks alone". The audit filed `.28` from
  the audit branch's older view, and no maintainer decision reconciles the two.
- Memory `project_shatter_git_hook_test_corrupts_worktree.md` (updated 09-07)
  still says "commit and push with `--no-verify`". `project_beads_git_hook_timeout.md`
  says `core.hooksPath=/dev/null`.
- Session data since 09-04 (`audits/2026-09-22/sessions/conv_recent.txt`):
  `--no-verify` 43 times in 10 sessions, `hooksPath=/dev/null` 15 times in 2
  sessions, and bd timeouts 24 times in 10 sessions. Bento's new
  `require-worktree-git-guard.py` now blocks some `--no-verify` calls
  ("Blocked: '--no-verify' skips git …"), so memory and hook now tell agents
  opposite things.

### AR-08 (P2, AGENT): shatter's Claude memory is stale, and some of it is harmful

Directory `~/.claude/projects/-home-ketan-project-shatter/memory/`:

| File | Problem |
|---|---|
| `project_shatter_gate_cache_and_bare_primary.md` + MEMORY.md index line | Says "primary checkout has core.bare=true … do not run git there". It is now `false`. |
| `project_audit_2026_09_04.md` | Front-matter description: "no bd issues were filed because bd was down". The body and the index say they were filed. |
| `project_shatter_git_hook_test_corrupts_worktree.md` | Prescribes `--no-verify` and claims "the primary checkout also has core.hooksPath=/dev/null" (no longer true). It never mentions the str-jttrf fix (09-12), which is the likely root cause of the 09-07 recurrence it describes. |
| `project_audit_2026_07_10_gate_state.md` | Not in MEMORY.md. Points at `audits/2026-07-10.md`, which never existed (str-sff87 closed). |
| `project_taskfile_migration.md` | Describes a finished migration as an 8-issue plan and cites `/tmp/taskfile-issues.md` and `.claude/plans/…`. |
| `project_rtk_wrapper_corrupts_redirects.md` | Describes a PreToolUse rewrite hook that `~/.claude/settings.json` no longer registers. AGENTS.md still says to prefix manually (AR-09). |
| `project_pickpackit_*`, `project_kapow_*`, `project_zolem_*`, `project_shatter_rust_single_file_analysis.md` (34.5 KB) | Other-project content, ~64 KB in total. Prior audit item 61 raised this; it was never filed as a tracker issue. |

The prior audit's memory recommendations (items 60–62) went into no tracker at
all, which explains why none of them happened.

### AR-09 (P2, L2/L3): the RTK block contradicts the rest of the guidance, and it swallowed a shatter section

- `AGENTS.md:528-594` sits between `<!-- headroom:rtk-instructions -->` markers
  and says "When running shell commands, **always prefix with `rtk`**",
  including `rtk find` and `rtk cargo test`. That conflicts with the task
  facade rule ("do not run … bare"), with the rtk `find` failures recorded in
  memory and sessions, and with the global rule that dedicated tools take
  precedence.
- The global `~/dotfiles/codex/AGENTS.md` says "its RTK precedence rule lives
  inside the `<!-- headroom:rtk-instructions -->` markers that rtk's own tooling
  manages per-repo". A grep for "precedence" in shatter's block (and in the
  dotfiles one) finds no such rule. dotfiles #3 and #10 are closed, but the fix
  never reached this repo.
- `## Shared-Machine Resource Etiquette (str-35vtk.5)` (lines 535-558) sits
  *inside* the rtk-managed markers, between the RTK intro and "Key Commands".
  An rtk re-sync of the block will silently delete the project's
  heavyweight-slot and parallelism-budget rules.
- The rtk rewrite hook (`~/.claude/hooks/rtk-rewrite.sh`) exists but is not
  registered in `~/.claude/settings.json`.

### AR-10 (P2, AGENT): a global gitignore hides `.claude/` and `.codex/`, so new agent files are silently untracked

- `git check-ignore -v --no-index .claude/skills/newskill/SKILL.md` →
  `/home/ketan/.config/git/ignore:35:.claude/`. Line 36 ignores `.codex/`.
  The repo `.gitignore` has no negation (it ignores only
  `.claude/settings.local.json` and `.claude/worktrees/`).
- The 16 tracked `.claude/**` files are tracked only because someone
  force-added them. A new skill, reference file or `.codex/AGENTS.md` (the
  decided `str-qwua7.54`) would not show in `git status` and would never land.
- `.codex/` in the primary checkout holds untracked symlinks. `.agents/` is
  empty. `.claude/worktrees/str-umw3/` is still an orphan (issue str-umw3
  closed 2026-04-11).

### AR-11 (P2, L2/AGENT): the landing verifier is unchanged, and bento's new guards cannot engage

- `scripts/land_work_verifier.sh:3` says "Runs the same gates ci.yml uses". It
  runs `task test-standard`, `task parity` and `task conformance`. CI runs
  `task check` plus shatter-llm.
- Every check is invoked as `"$@" >/dev/null 2>&1`, so bento-rdtn.4's persisted
  verifier log is empty whenever a check fails. The script never emits
  `executed` or `wall_seconds`, so bento-rdtn.6's all-cached guard
  (`land-work-run-verifier.py:525-541`) cannot trigger ("a v1 payload with no
  executed field anywhere is … unaffected"). `verifier.json` has no timeout.
- The root CLAUDE.md says "Solo agents that merge their own branch into
  `main` run the full check before pushing `main`". Close reasons show landings
  gated only by "land.py verify step passed on full task test-standard"
  (str-qwua7.4, .7, str-0m0vn, str-vr7vq, str-rmcrl, str-na9db, str-6vl7p).
  str-qyi6l was gated by bare `cargo test -p shatter-core --lib`.
- Sessions since 09-04: 65 "task … is up to date" results in 24 sessions.
  `/pre-completion` still has no executed-or-cached column.
- Prior refs: str-qwua7.2 and .55, both open. The adoption half of bento-rdtn.4/.6
  was never filed on the shatter side.

### AR-12 (P2, L2): AGENTS.md landing procedures contradict the shipped bento driver

- `AGENTS.md:104-133` ("Landing the Plane") prescribes `git checkout main`,
  `git push --force-with-lease` and `git merge --no-ff` by hand.
  `AGENTS.md:515-521` says "Never run `git worktree remove`". Meanwhile bento
  shipped `land.py` (bento-rdtn.14; sessions invoke it 28–44 times since
  09-04), and bento-rdtn.15's `require-worktree-git-guard.py` blocks
  branch-mutating git in the primary checkout.
- Consequences visible in git: 37 remote branches fully merged into origin/main
  are not deleted (AGENTS.md step 5 says deleting them is mandatory). 28
  unmerged remote branches include the superseded originals of re-landed work
  (`str-qwua7.4-testplan-http-body-fix`: 1 cherry-unique commit;
  `str-qwua7.7-protocol-registry-validate`: 4; `str-gjsb2-…`: 1). Two local
  `recovery/*` branches from 09-12 remain.
- Prior ref: str-qwua7.23 (open, not started). AGENTS.md is still 30,731 bytes,
  and no agent doc has been edited since 09-07.

### AR-13 (P2, L2): the frontend-parity skill's capability table contradicts the matrix it calls authoritative

- `.claude/skills/frontend-parity/SKILL.md:37` shows Go `thrown_error` as ✗
  ("panics handled internally, not surfaced"). `protocol/parity-matrix.yaml:509-518`
  says `go: captured`.
- Line 59 says "Rust's analyze handler is a stub", and line 69 says the Rust
  timeout is "stored, not yet applied — execute unimpl". The Rust frontend
  executes: `e2e_concolic_rust.rs` exists, and close reasons cite 587 passing
  shatter-rust tests.
- Line 27 of the same skill says "trust the matrix". Prior ref: str-qwua7.24.

### AR-14 (P2, AGENT): repo skills have rotted against the task facade

- `check-go`, `check-rust` and `check-ts` run bare `go test ./...`,
  `cargo test` and `npm test`. CLAUDE.md and `check-all` both say "Do not rerun
  their underlying commands bare". Nothing references these three skills.
- `protocol-sync` compares three hand-written files by eye. It ignores
  `protocol/registry.yaml`, the generated bindings and the Rust frontend, and
  `task parity` / `protocol-codegen` already do this mechanically.
- `audit` Phase 1 uses bare `cargo test`, `npm test` and `go test`. Phase 4
  names `GLOSSARY.md` at the root, but the file is `docs/GLOSSARY.md`. Phase 7
  reads "last 20 commits". Post-audit step 6 commits on main and defers to
  `bd sync`.
- `bugfix` lines 29-31, 60-62 and 67-70 use bare commands as well.
- Positive: every backticked `task <name>` in AGENTS.md, CLAUDE.md (root and
  crates), skills, README, CONTRIBUTING and DRIFT-PATROL resolves against
  `task --list-all` (97 tasks). Every `scripts/`, `demo/`, `docs/` and
  `protocol/` path resolves. The drift lint (str-u394l.4) would therefore be
  cheap to add as a gate.

### AR-15 (P2, AGENT): doctor nudges repeat every session and nobody acts on them

- The bento `agent-env-doctor.py` SessionStart output today, in both the
  primary checkout and the audit worktree: "storystore dormant — decision
  pending", "bugshot dormant — decision pending", and 5 orphan worktree
  directories (`str-6q1i`, `str-hszo-tmpfix`, `str-k6e61-scm-followups`,
  `str-mambd-enum-variant-gen`, `str-yhsp-concolic-run`, all dating from
  June–July).
- The maintainer decided on 2026-09-06 (report §13, items 9–10): adopt
  storystore, and silence bugshot until bgs-3tq. `.agent-mode.local` still
  holds only `dangerous`, `agent_env_doctor_seen=bugshot,storystore` and
  `…superpowers_pointer_seen=true`. The decision was never recorded, even
  though bento-rdtn.2 shipped a way to record it.
- The doctor does not flag `.claude/worktrees/str-umw3`, the poisoned
  `user.name` (AR-01), local `recovery/*` branches, or red scheduled workflows.

### AR-16 (P2, AGENT): a fixture-corruption incident was never tracked, and one issue was closed against the corrupted commit

- `recovery/shatter-main-20260912-11_4ty6m` points at e50fc399, "init" by
  `Test <test@example.com>` on 2026-09-07. That is a fixture commit on the real
  local main: 82 files changed, −12,090 lines, including a restore of the
  deleted `shatter-vs/` and `.claude/agents/`. `git merge-base --is-ancestor
  e50fc399 origin/main` → NOT_IN_MAIN.
- `str-qwua7.14` was closed on 09-08 as "Not reproducible against current main
  (e50fc399)". The diagnosis ran on the corrupted HEAD and called it main.
- `bd search recovery` finds nothing. The recovery branches have sat for 10
  days without review.

### AR-17 (P3, AGENT): the git log shows landing churn and uninformative merges

- 6 merges titled `Merge commit '<sha>' into HEAD` (09-07/08: af6839f0,
  a19a8aec, 2aecd43a, c1364378, af3ae54a, 6c8bc87f). These are improvised
  preview-worktree landings whose messages name neither branch nor issue.
- `Merge branch 'str-qwua7.4-landing2'`, and str-qwua7.7 "ported/rewritten
  against current main … rather than the stale original diff": work was
  re-landed because the branches went stale.
- Parked branches from 08-27 to 08-31 landed on 09-21/22, three to four weeks
  after they were written.
- One revert of an "unauthorized" edit (str-mpgg1) happened after two
  concurrent sessions took opposite decisions on the same branch.
- Positive: from 09-19 on, the land.py merges are uniform (`Merge branch
  '<branch>'`), and "address review" commits show that review happens before
  landing.

### AR-18 (P2, AGENT): priority inflation and no triage cadence

- `bd list --status open` shows P1 45, P2 111, P3 21. 12 of the P1s predate
  August. `bd ready` returns 154 issues, and its first 25 are all P1.
- The `agents` label has 12 open issues, all P2. None was touched after
  filing. `str-qwua7.62` (the tracker sweep maintainers approved on 09-06) is
  open.
- A new P1 epic (str-hjrnp) went from plan to closed in about 24 hours on
  09-21/22, while 16 prior-audit P1s stayed ready.

### AR-19 (P3, AGENT/L3): the swarm config is in a format bento no longer reads

- `.claude/swarm-config.md` sets the swarm gates (`/check-all`,
  `/pre-completion`, `/walkthrough-review`). bento
  `swarm/scripts/swarm-discover.py:18-21` reads only `swarm-config.json` (repo
  root, `.claude/`, or `.codex/`). No such file exists, so swarms run in serial
  mode, and CLAUDE.md's "The lead runs one full `task check` at batch landing"
  has no config behind it.
- `check-all` has `disable-model-invocation: true`, so a swarm lead following
  that config cannot run it through the Skill tool.

## 2. Positives worth preserving

- Every backticked `task`, script and path reference in the agent docs
  resolves (see AR-14). The root CLAUDE.md's parallel-path names
  (`buildSymExprWithFlow`, `--concolic`, `examples/go/05-conditional-merge.go`,
  `e2e_concolic_go.rs`) are accurate.
- `scripts/drift-patrol.py` is well designed: PENDING checks linked to live
  issue ids, a failure when PENDING outlives its issue, and paste-ready
  remediation. Its local run surfaced real problems. The CI wiring (AR-02) is
  the only broken part.
- `str-jttrf` fixed the fixture leak test-first, with a
  `test_git_fixture_isolation.py` regression that `task meta` runs.
- Landings through bento `land.py` since 09-19 record gate evidence, verify
  time and review results in close reasons.
- bento and dotfiles acted on every item the prior audit filed against them
  within about 3 days.
- The per-crate CLAUDE.md inventory in the root CLAUDE.md is accurate (exactly
  five crates carry one).

## 3. Grades

| Level | Grade | Basis |
|---|---|---|
| AGENT | D+ | The forcing functions (patrol CI, release CI, doctor) are red or ignored; audit follow-through failed for the third time; memory and AGENTS.md prescribe bypasses. Offset by strong bento/dotfiles follow-through and good land.py evidence. |
| L3 (agent docs) | C- | AGENTS.md is stale (bd sync, manual landing, rtk block eating a section); no agent-doc edits since 09-07; references resolve. |
| L2 (agent docs vs code) | C- | Verifier header, frontend-parity table, `bd sync` and swarm config all disagree with reality. |
