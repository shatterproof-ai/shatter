# Issue bundle: bento-guards-doctor-tracker

- Audit: shatter audit 2026-09-22
- Bucket: bento-guards-doctor-tracker
- Repo: bento (tracker: bd in /home/ketan/project/bento, prefix bento)
- Parent epic: Epic: Audit 2026-09-22 findings (bento)
- Theme: guards, doctor and tracker flow (git-guard bypasses, git-hook latency visibility, beads Dolt-remote guidance, check-unpushed, doctor state, previews, closure, close evidence, cross-check hook reentrancy)
- Status: drafts only. Nothing is filed (D6).
- Code facts re-verified against bento origin/main @ 0b8d488 (2026-09-23). Since b1bb787 only `check-unpushed.py` (3731a6f, bento-2p2p) and cross-check (96e3122) changed among cited files; check-unpushed line numbers were updated.
- Revised 2026-09-23 after the Codex cross-check (`issues/crosscheck/bento-guards-doctor-tracker.codex.md`) and a same-runtime review; overlaps were verified with `bd show` against live bento tracker state. See REVISION.md.

## Maintainer decisions (2026-09-23)

- D1 Releases: keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix; fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64). Release work closes only with a green release-run URL.
- D2 shatter diff: retire the snapshot-diff command and the unused Snapshot writer path; spec-diff is the regression tool. Update SPEC/README/QUICKSTART. The `diff` name becomes free; str-81xiw decides its reuse. Correct shatter-agents' `shatter diff --staged` docs.
- D3 Concolic positioning: measure first. P1 controlled default-vs-concolic benchmark; P1 fix concolic early termination; a follow-up decision issue re-decides "concolic-first" positioning. No doc softening now.
- D4 Beads hook stall: bd's post-checkout hook spends ~6 min importing .beads/issues.jsonl (waiting, not computing); the JSONL is an export, not sync. Retire the JSONL import in shatter; move tracker sync to a Dolt remote; verify first whether stale imports clobbered newer DB state; AGENTS.md drops `bd sync`; str-qwua7.28 superseded; bento beads-issue-flow gets matching guidance. No BEADS_HOOK_TIMEOUT fix and no hook-bypass guidance anywhere.
- D5 Git identity: the leaked [user] section is already removed. Drafts cover a .mailmap, a git-state check, and a fixture .git/config snapshot test.
- D6 Filing: after reconciliation and the Codex cross-check the maintainer runs one filer script. Nothing is filed by agents.

D4 shapes this bucket: git-hook-latency-visibility (warn while slow and point to guidance; no bypass, no timeout env var) and beads-dolt-remote-guidance (P1). D1, D2, D3 and D5 do not touch this bucket.

## Dependency edges (by slug)

- git-guard-bypasses-and-false-positives is blocked by bento-l01v (via l01v-residual-bypasses-note) and bento-i76i (via i76i-switch-update-ref-note).
- git-hook-latency-visibility is blocked by beads-dolt-remote-guidance and by land-py-invocation-progress-log (bucket bento-landing).
- beads-dolt-remote-guidance is blocked by bento-49pg (via 49pg-dolt-remote-section-note).
- close-reason-evidence is blocked by bento-wzbt (via wzbt-manual-close-note); followups-as-siblings is blocked by close-reason-evidence.
- Reopen notes git-guard-reopen-note and doctor-state-reopen-note are posted after their follow-ups are filed (ordering only).
- followups-as-siblings carries a companion comment for bento-x4bm.

## Contents

- 01-git-guard-bypasses-and-false-positives.md — P1 — new — Git guard: close the bypasses bento-l01v leaves open (/usr/bin/git, timeout/nice/ionice/sudo wrappers, -C <path>, earlier cd, GIT_CONFIG_* env)
- 02-git-guard-reopen-note.md — P1 — reopen-note on bento-rdtn.15 — Note on closed bento-rdtn.15: guard is bypassed by /usr/bin/git, wrapper prefixes, -C, cd and GIT_CONFIG env
- 03-git-hook-latency-visibility.md — P1 — new — launch-work and land-work: warn while a git subprocess is slow and name its hooks; guard's slow-hook pointer must cite real guidance; doctor flags stale beads hook markers
- 04-beads-dolt-remote-guidance.md — P1 — new — beads-issue-flow: "Snapshot and Dolt remote" section (no JSONL import on checkout, Dolt remote for cross-clone sync, no bd sync); doctor checks import-on-checkout and bd sync mentions
- 05-check-unpushed-overcount-and-blocks.md — P2 — new — check-unpushed Stop hook: count against all remotes (not @{u}), report without blocking on the primary branch in the primary checkout, and do not block a branch that a live land.py is landing
- 06-doctor-state-per-worktree.md — P2 — new — Resolve .agent-mode.local repo-wide (main working tree), with a locked writer, so linked-worktree sessions see collapsed doctor state and primary-checkout decisions
- 07-doctor-state-reopen-note.md — P2 — reopen-note on bento-rdtn.2 — Note on closed bento-rdtn.2: doctor seen/decided state is per checkout, so linked worktrees get full nudges
- 08-stale-previews-leak-and-scoping.md — P2 — new — Stale land-work previews: bento test suite leaks previews into /tmp; preview creation ignores TMPDIR; doctor discovery is unscoped, /tmp-only, and names a closure mode that does not exist
- 09-closure-orphan-worktree-dirs.md — P2 — note-to-existing on bento-nljv — Note on bento-nljv: audit 2026-09-22 confirms the five shatter orphan dirs persist; add a build-output-only classification so non-empty orphans get a reviewable, per-path cleanup
- 10-close-reason-evidence.md — P2 — new — beads-issue-flow: close helper for closes that are not landings (not reproducible, duplicate, wontfix, superseded) with typed reason validation and SHA ancestry checks, plus a report of non-conforming close reasons
- 11-claim-branch-reconciliation.md — P2 — new — closure: report stale in_progress claims (no branch/worktree, or branch already merged into the primary branch)
- 12-followups-as-siblings.md — P3 — new — Review follow-ups should be filed as siblings with discovered-from, not children of the issue being closed; the non-landing close helper refuses parents with open children
- 13-per-subagent-scratch-dirs.md — P3 — new — code-bloat-sniffer and swarm prompts: give each parallel subagent or teammate its own scratch subdirectory and forbid rm -rf outside it
- 14-a0nz-behavioural-probe.md — P3 — note-to-existing on bento-a0nz — Note on bento-a0nz: audit grades need claim-appropriate evidence (production reachability for wiring claims, default-configuration probes for behaviour claims); add a Rust reachability mechanism; build the CLI from the audited SHA
- 15-cross-check-stop-hook-hijack.md — P1 — new — cross-check: bento Stop hook hijacks the read-only Codex reviewer's final message; identity check then fails, or passes on an empty review
- 16-l01v-residual-bypasses-note.md — P1 — note-to-existing on bento-l01v — Note on bento-l01v: residual guard bypasses (/usr/bin/git, timeout/nice/ionice/sudo, -C, earlier cd, GIT_CONFIG_* env) are tracked separately and build on shell_segments.py
- 17-i76i-switch-update-ref-note.md — P2 — note-to-existing on bento-i76i — Note on bento-i76i: also deny `git switch <primary>` and `git update-ref refs/heads/<primary>` in the primary checkout
- 18-49pg-dolt-remote-section-note.md — P2 — note-to-existing on bento-49pg — Note on bento-49pg: a follow-up adds the beads-issue-flow "Snapshot and Dolt remote" section your one-sentence rule can point to
- 19-wzbt-manual-close-note.md — P2 — note-to-existing on bento-wzbt — Note on bento-wzbt: a follow-up adds a close helper for non-landing closes (not reproducible, duplicate, superseded, wontfix) that reuses this contract's shared syntax

---

<!-- 01-git-guard-bypasses-and-false-positives.md -->

---
slug: git-guard-bypasses-and-false-positives
kind: new
title: "Git guard: close the bypasses bento-l01v leaves open (/usr/bin/git, timeout/nice/ionice/sudo wrappers, -C <path>, earlier cd, GIT_CONFIG_* env)"
priority: P1
type: bug
labels: [audit, hooks, safety]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: [l01v-residual-bypasses-note, i76i-switch-update-ref-note]
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Git guard: close the bypasses bento-l01v leaves open (/usr/bin/git, timeout/nice/ionice/sudo wrappers, -C <path>, earlier cd, GIT_CONFIG_* env)

Follow-up to closed bento-rdtn.15, whose goal is not met. Source findings: sessions-02, bento-04, sessions-15 (shatter audit 2026-09-22).

Related, and the scope boundary for this issue:

- **bento-l01v** (open): adds `shell_segments.py`, a quote-, heredoc- and comment-aware segmenter that also strips the `rtk`, `command`, `exec` and `env` wrappers. It fixes every false positive and the `rtk`/`command`/`env` false negatives. It lists "a `cd` in an earlier segment does not change how paths are resolved later" as a known limit. **This issue builds on `shell_segments.py`; it must not add a second tokenizer.**
- **bento-i76i** (open): adds commit/cherry-pick/pull/am/revert on the primary branch and clustered `-n`. It explicitly keeps the `-C <path>` limitation ("repo_root always comes from the payload cwd"). The `switch <primary>` and `update-ref refs/heads/<primary>` verbs this audit found are handed to i76i by a note (i76i owns the primary-branch verb list); they are not in this issue.
- **Landing order** (l01v's shared-parser order): bento-l01v, then bento-1srw (Codex copy of the guard), then bento-i76i, then bento-nfi3, then this issue. If the Codex copy exists when this lands, update both runtime copies (`tests/test_vendored_parity.py`).
- bento-rdtn.15 (closed; a reopen note points here).

## Problem

bento-rdtn.15 added `require-worktree-git-guard.py`, a PreToolUse Bash hook that blocks branch-mutating git in the primary checkout and hook bypasses (`--no-verify`, `-c core.hooksPath=`) everywhere. After bento-l01v and bento-i76i land, the guard will still miss these spellings, which are the ones agents in shatter actually use:

- **Absolute git path.** In shatter session c1689435 (2026-09-21 23:03 to 09-22 02:00), after the guard shipped, about 22 commits and 18 pushes were spelled `/usr/bin/git -c core.hooksPath=/dev/null commit|push ...`. Shatter's own memory tells agents to use `/usr/bin/git` to avoid the rtk wrapper, so this is the common form, not an edge case. l01v requires `argv[0] == "git"`.
- **Wrappers l01v does not strip:** `timeout N`, `nice [-n N]`, `ionice [-c N] [-n N]`, `sudo [-u U]`.
- **Repo resolution.** `git -C <primary> merge x` run from elsewhere, and `cd <primary> && git merge x`, are judged against the payload cwd, not the repo git would act on.
- **Config via environment.** `GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0=/dev/null git commit` and `GIT_CONFIG_PARAMETERS="'core.hookspath'='/dev/null'" git commit` disable hooks without any `-c` or `--no-verify` token. l01v strips `VAR=value` prefixes (including after `env`) before the guard sees argv, so the guard cannot inspect them today.

## Evidence

Synthetic PreToolUse payloads piped into the guard at bento origin/main 0b8d488 (the guard is unchanged since b1bb787). Exit 0 = allowed; each of these should be 2:

- `/usr/bin/git commit --no-verify -m x`
- `/usr/bin/git -c core.hooksPath=/dev/null commit -m x`
- `timeout 60 git push --no-verify`
- `nice -n 5 git commit --no-verify -m x`
- `git -C <primary> merge feature` (payload cwd = a linked worktree)
- `cd <primary> && git merge feature` (payload cwd = a linked worktree)
- `GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0=/dev/null git commit -m x`

Controls, already exit 2: `git commit --no-verify -m x`, `git -c core.hooksPath=/dev/null push`, `git merge feature` with cwd = primary.

## Current code facts (bento origin/main @ 0b8d488; guard unchanged since b1bb787)

- Source: `catalog/hooks/bento/claude/scripts/require-worktree-git-guard.py`. The generated copy `plugins/claude/bento/hooks/scripts/require-worktree-git-guard.py` is rebuilt with `scripts/build-plugins`, never edited by hand.
- Lines 132-146: `_find_git_segments` skips leading `VAR=value` tokens without inspecting them, then requires `tokens[idx] == "git"`.
- Lines 101-129: `_parse_git_invocation` skips the path after `-C`; line 218 takes `repo_root` from the payload cwd.
- Line 207: any command containing `LAND_WORK_MARKER` (`BENTO_LAND_WORK=1`, line 42) is exempted wholesale. Unchanged by this issue.
- Lines 229-236: the hook-bypass block message's slow-hook pointer is fixed by git-hook-latency-visibility, not here.

## Acceptance criteria

- Each command in the Evidence list exits 2 with the existing message style; each control still exits 2.
- Negative cases exit 0: `/usr/bin/git status`, `timeout 60 git fetch`, `git -C <linked-worktree> merge x` from the primary checkout, `cd /tmp && git status`, `GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=user.name GIT_CONFIG_VALUE_0=x git commit -m x` in a linked worktree.
- `-C` resolution: a relative `-C` path resolves against the payload cwd (or the last effective `cd`), several `-C` flags compose as git composes them, and a path that does not exist fails open (exit 0), matching the guard's existing fail-open contract. Tests cover each.
- `cd` tracking applies only within one `&&`/`;` chain; a `cd` inside a subshell or `$(...)` does not leak out. Tests cover both.
- The environment rule reads the `VAR=value` assignments the segmenter strips. `shell_segments.command_segments` (or a sibling function) exposes them; l01v's corpus still passes unchanged.
- No second tokenizer: the diff imports `shell_segments.py` and adds no quote or heredoc parsing of its own (reviewer checks; a grep test asserts `require-worktree-git-guard.py` contains no `shlex.split` call).
- Proof at close: the close note names the new tests and records a run of them failing on the pre-fix guard (with l01v and i76i landed) and passing after, plus `scripts/build-plugins` reporting the generated copy in sync.

## Suggested approach

- Treat `os.path.basename(argv[0]) == "git"` as git.
- Extend l01v's prefix stripping with `timeout [opts] N`, `nice [-n N]`, `ionice [opts]`, `sudo [opts]`.
- Resolve the effective repo from `-C` and a tracked `cd` target, then run the existing primary-checkout test on that path.
- Treat `GIT_CONFIG_KEY_n=core.hooksPath` (case-insensitive key) and `core.hookspath` inside `GIT_CONFIG_PARAMETERS` like `-c core.hooksPath=`.

## Out of scope

- False positives and the `rtk`/`command`/`env` wrappers (bento-l01v).
- New primary-branch verbs, including `switch` and `update-ref` (bento-i76i; see the note filed there).
- The block message's slow-hook pointer (git-hook-latency-visibility).
- The per-checkout scope of the `hook_bypass=allow` opt-out (doctor-state-per-worktree).
- `bash -c '...'` and `eval` bodies (l01v known limit).

## Priority / Type / Labels

P1 / bug / audit, hooks, safety

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

Blocked by bento-l01v (supplies `shell_segments.py`) and ordered after bento-i76i (same file, shared landing order). Both edges come from the note drafts l01v-residual-bypasses-note and i76i-switch-update-ref-note, which resolve to those ids.

---

<!-- 02-git-guard-reopen-note.md -->

---
slug: git-guard-reopen-note
kind: reopen-note
title: "Note on closed bento-rdtn.15: guard is bypassed by /usr/bin/git, wrapper prefixes, -C, cd and GIT_CONFIG env"
priority: P1
type: note
labels: [audit, hooks, safety]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: [git-guard-bypasses-and-false-positives]
existing_id: bento-rdtn.15
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-rdtn.15

Target: bento-rdtn.15 (closed). Action: add a comment (`bd comments add bento-rdtn.15 ...`). Do not reopen; the follow-up work is tracked in the new issue. Post it after git-guard-bypasses-and-false-positives is filed, and replace the slug with that issue's id.

## Comment text

Audit 2026-09-22 (shatter; findings sessions-02, bento-04): the goal of this issue is not met in practice. `require-worktree-git-guard.py` only inspects segments whose first non-`VAR=value` token is literally `git`, so these all exit 0:

- `/usr/bin/git -c core.hooksPath=/dev/null commit ...`
- `rtk git commit --no-verify`, `timeout 60 git push --no-verify`, `env git ...`, `nice -n 5 git commit --no-verify`
- `git -C <primary> merge ...` and `cd <primary> && git merge ...`
- `GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0=/dev/null git commit`

In shatter session c1689435 (2026-09-21/22), after this guard shipped, about 22 commits and 18 pushes used the `/usr/bin/git -c core.hooksPath=/dev/null` form (re-probed at bento 0b8d488: all of the above still exit 0).

Where the fixes live: the quote/heredoc false positives and the `rtk`/`command`/`env` wrappers are bento-l01v; new primary-branch verbs (including `switch` and `update-ref`) are bento-i76i; `/usr/bin/git`, `timeout`/`nice`/`ionice`/`sudo`, `-C`, earlier `cd` and `GIT_CONFIG_*` env are <id of git-guard-bypasses-and-false-positives>.

---

<!-- 03-git-hook-latency-visibility.md -->

---
slug: git-hook-latency-visibility
kind: new
title: "launch-work and land-work: warn while a git subprocess is slow and name its hooks; guard's slow-hook pointer must cite real guidance; doctor flags stale beads hook markers"
priority: P1
type: bug
labels: [audit, land-work, launch-work, hooks]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: [beads-dolt-remote-guidance, land-py-invocation-progress-log]
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# launch-work and land-work: warn while a git subprocess is slow and name its hooks; guard's slow-hook pointer must cite real guidance; doctor flags stale beads hook markers

Source findings: bento-03 (also sessions-05), shatter audit 2026-09-22. Maintainer decision D4 (2026-09-23) applies. Related shatter issue: beads-retire-jsonl-import-dolt-remote (the shatter-side root-cause fix; mention in the body only, it is not a bento id). Related bento draft in this epic: land-py-invocation-progress-log (bucket bento-landing), which makes land.py run children with `Popen` and emit heartbeats; this issue relies on that plumbing so child warnings reach the user.

## Problem

In shatter, git hooks installed by beads stall every worktree creation and every landing for minutes. Bento scripts that create worktrees pay that cost silently and in series (`launch-work-bootstrap --apply`, `land-work-create-preview.py`, merge/push in `land.py`). Agents see a hung command with no explanation and reach for hook bypasses. The bento git guard blocks the bypass, but its message sends them to "the launch-work skill's dependency-bootstrap guidance for slow-hook fixes", and that reference has no hook content.

The root cause was measured on 2026-09-23 (D4): bd's post-checkout hook spends about 6 minutes "importing JSONL from .beads/issues.jsonl" (1,773 issues, about 10 s of CPU, so it is waiting, not computing). The fix for that is the shatter issue beads-retire-jsonl-import-dolt-remote, plus the bento guidance in beads-dolt-remote-guidance. This issue makes the cost visible in bento's tools **while the wait is happening** and points agents at the right fix. It deliberately adds no hook bypass and no timeout override.

## Evidence (shatter, 2026-09-19..23)

- `land.py` `create_preview` took 234.9-301.2 s in 11 successful landings (one 10.9 s outlier on 09-21).
- Every `launch-work-bootstrap --apply` exceeded the Bash tool timeout: more than 120 s on 09-19, more than 600 s on 09-21, more than 120 s for this audit's launch.
- Transcripts contain "beads: hook 'post-checkout' timed out after 300s" 32 times across 7 sessions.
- The installed hook `$(git rev-parse --git-common-dir)/hooks/post-checkout` in shatter carries `# --- BEGIN BEADS INTEGRATION v0.63.3 ---`, and `.beads/hooks/post-checkout` carries `# bd-hooks-version: 0.56.1`. `bd version` reports 1.1.0 (checked 2026-09-23).
- `time bd hooks run post-checkout` in a shatter linked worktree took 2m59.7s.

## Current code facts (bento origin/main @ 0b8d488; cited files unchanged since b1bb787)

- `catalog/skills/launch-work/scripts/launch-work-bootstrap.py` lines 301-314: runs `git worktree add -b ...` with no timing or warning.
- `catalog/skills/land-work/scripts/land-work-create-preview.py` line 343: `git("worktree", "add", "--detach", ...)` for the preview, untimed.
- `catalog/skills/land-work/scripts/land.py` `_run_script` (line 105): runs each step with `subprocess.run(..., capture_output=True)` and only surfaces the child's stderr inside a `StepFailure` message. On success the child's stderr is discarded, so a warning printed by `land-work-create-preview.py` today would never reach the user, and nothing is shown until the step returns.
- `catalog/hooks/bento/claude/scripts/require-worktree-git-guard.py` lines 229-236: the block message cites "the launch-work skill's dependency-bootstrap guidance for slow-hook fixes".
- `catalog/skills/launch-work/references/dependency-bootstrap.md`: zero matches for "hook" or "slow".
- `catalog/hooks/bento/claude/scripts/agent-env-doctor.py`: has a hook-binaries check (`check_hook_binaries`, line 389) but no hook-latency or beads hook-version check.

## Acceptance criteria

- **Live warning, not a post-mortem.** `launch-work-bootstrap`, `land-work-create-preview.py` and the merge/push subprocesses in `land.py` run each hook-triggering git command through one helper. When the command is still running after the threshold (default 30 s), the helper prints, while it is still running, one line naming the command, the hook(s) that git will run for it (resolved from `git rev-parse --git-path hooks`, which honours `core.hooksPath`), whether a hook is beads-managed (contains a `BEADS INTEGRATION` or `bd-hooks-version` marker), and a pointer to the beads-issue-flow "Snapshot and Dolt remote" section. On completion over the threshold it prints the elapsed time.
- **Reaches the user through land.py.** A test runs `land.py` end to end against a fixture repo whose `post-checkout` hook sleeps past a test threshold, and asserts that the warning line appears on land.py's stderr (and in its progress log from land-py-invocation-progress-log) **before** the `create_preview` step finishes. A test that only calls `land-work-create-preview.py` directly does not satisfy this criterion.
- **Test threshold without env vars.** The threshold is a helper parameter and a CLI flag on the scripts (for example `--slow-git-warn-seconds`), so tests run in a few seconds. No environment variable is added (D4 forbids hook-timeout env vars; this keeps the surface unambiguous). A fast hook produces no warning.
- **Guard pointer.** The guard's hook-bypass block message cites a section that exists and discusses slow hooks: the beads-issue-flow "Snapshot and Dolt remote" guidance from beads-dolt-remote-guidance. A test asserts the cited file and heading exist in the catalog.
- **Stale beads hook markers.** The doctor reports when a repo's beads hook markers are older than the installed bd: it parses `BEADS INTEGRATION vX.Y.Z` in the effective hooks dir and `bd-hooks-version: X.Y.Z` in `.beads/hooks`, parses `bd version`, and warns when either marker's (major, minor) is lower than bd's. It names `bd hooks install` as the refresh step. `bd version` runs with a 5 s timeout; on timeout or absence the check is skipped silently. Fixture tests cover older, equal and newer markers, and a missing bd.
- Nothing added by this issue sets `BEADS_HOOK_TIMEOUT`, `core.hooksPath`, `--no-verify` or any other hook bypass or timeout override, and no guidance recommends one. The helper passes the caller's environment and git config through unchanged; a test asserts the child git process sees no added `GIT_CONFIG_*`, `-c` or `BEADS_*` settings. Reviewers confirm the prose.
- Proof at close: the close note names the new tests with failing-then-passing runs, and includes the stderr of one real `land.py` or `launch-work-bootstrap --apply` run in a repo with a slow hook, showing the warning line timestamped before the step completed.

## Suggested approach

- A small `timed_git(...)` helper in the shared script utilities using `Popen` plus `wait(timeout=threshold)`, printing the warning on the first timeout and continuing to wait.
- For `land.py`, reuse the `Popen`/heartbeat loop from land-py-invocation-progress-log and forward child stderr lines as they arrive, instead of discarding them.
- Warn only; do not change hook behaviour for previews or work worktrees.

## Out of scope

- Any hook bypass, timeout override, or hydration suppression (D4).
- Shatter's own hook and JSONL import changes (shatter beads-retire-jsonl-import-dolt-remote).
- Fixing bd hook latency upstream.
- The guard's bypass detection (git-guard-bypasses-and-false-positives, bento-l01v).
- land.py's progress log and heartbeat themselves (land-py-invocation-progress-log).

## Priority / Type / Labels

P1 / bug / audit, land-work, launch-work, hooks

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

- Blocked by beads-dolt-remote-guidance: the guard pointer and the warning must cite the section that issue adds.
- Blocked by land-py-invocation-progress-log (bucket bento-landing): the land.py part needs its `Popen`/heartbeat plumbing so child output is visible while a step runs.
- The launch-work timing and the doctor marker check can start earlier.

---

<!-- 04-beads-dolt-remote-guidance.md -->

---
slug: beads-dolt-remote-guidance
kind: new
title: "beads-issue-flow: \"Snapshot and Dolt remote\" section (no JSONL import on checkout, Dolt remote for cross-clone sync, no bd sync); doctor checks import-on-checkout and bd sync mentions"
priority: P1
type: task
labels: [audit, beads-issue-flow, hooks]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: [49pg-dolt-remote-section-note]
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# beads-issue-flow: "Snapshot and Dolt remote" section (no JSONL import on checkout, Dolt remote for cross-clone sync, no bd sync); doctor checks import-on-checkout and bd sync mentions

Source findings: bento-16, prior-03 (context), shatter audit 2026-09-22. Maintainer decision D4 (2026-09-23) applies. Raised to P1 because git-hook-latency-visibility points agents at the section this issue adds.

Related:

- **bento-49pg** (open, owner decision option C, 2026-09-22): untracks the export, puts one sentence ("The Beads JSONL export ... is untracked local state ... sync tracker state with `bd dolt push` / `bd dolt pull`") into both land-work "Tracker Handoff" (L785-788) and beads-issue-flow (L233-234), and adds `check_tracked_beads_export` to both doctors. **Those edits and that doctor check belong to 49pg and are not repeated here.** This issue adds the longer section that 49pg's sentence can point to, and two doctor checks 49pg does not cover.
- bento-rdtn.12 (closed): documented the bd CLI surface but not export or remote setup.
- Shatter issues beads-retire-jsonl-import-dolt-remote and beads-jsonl-consumers-drop-bd-sync (mention in the body only; not bento ids).

## Problem

bento gives consuming repos no guidance on how beads state moves between clones and machines under bd 1.x, and repos have filled the gap with patterns bd no longer supports:

- **JSONL import on checkout.** In shatter, bd's post-checkout hook spends about 6 minutes "importing JSONL from .beads/issues.jsonl" (1,773 issues, about 10 s CPU) on every worktree creation, checkout and landing preview (measured 2026-09-23, D4). bd itself warns that the JSONL "is an export, not cross-machine sync or source of truth" and suggests `bd dolt remote add origin ... && bd dolt push`. Importing a stale snapshot may also overwrite newer database state (shatter is verifying that in beads-retire-jsonl-import-dolt-remote).
- **`bd sync`.** bd 1.1.0 has no `bd sync`, but consumer docs still require it. Shatter's AGENTS.md mentions it 10 times, including a landing step.

Important scope correction: **linked worktrees do not need any sync.** `bd worktree --help` (bd 1.1.0) says "Worktrees automatically share the same beads database as the main repository via git common directory discovery". A Dolt remote is only needed to move tracker state between separate clones or machines. The JSONL import on checkout is therefore pure cost in a worktree-based workflow.

## Evidence (re-verified 2026-09-23)

- `bd sync --help` gives `Error: unknown command "sync" for "bd"` (bd 1.1.0).
- `bd dolt --help` lists `bd dolt remote add <name> <url>`, `bd dolt remote list`, `bd dolt push` and `bd dolt pull`.
- `bd worktree --help`: linked worktrees share the main repository's database.
- `grep -c 'bd sync' AGENTS.md` in shatter: 10.
- Hooks print "post-checkout JSONL import warning: no Dolt remote configured" (26 times in shatter transcripts).

## Current code facts (bento origin/main @ 0b8d488; cited files unchanged since b1bb787)

- `catalog/skills/beads-issue-flow/SKILL.md` has no JSONL-export or Dolt-remote section. Line 19 says "Never read `.beads/`, `issues.jsonl`, or the Dolt ... directly".
- `catalog/hooks/bento/claude/scripts/agent-env-doctor.py` and `catalog/hooks/bento/codex/scripts/agent-env-doctor.py` (separate, non-identical files) have no beads import-on-checkout or `bd sync` check.

## Acceptance criteria

- beads-issue-flow gains a section headed exactly `## Snapshot and Dolt remote` (git-hook-latency-visibility cites this heading) that states:
  - `.beads/issues.jsonl` is an export, not sync or source of truth; agents and scripts must not treat it as current tracker state (consistent with, and linking to, 49pg's one-sentence rule rather than restating a different one);
  - linked worktrees of one clone share one database and need no sync step;
  - repos should not import the JSONL on checkout (no JSONL import in post-checkout/post-merge hooks), and how to check whether a repo does;
  - moving tracker state between separate clones or machines uses a Dolt remote: `bd dolt remote list`, `bd dolt remote add origin <url>`, `bd dolt push`, `bd dolt pull`;
  - `bd sync` does not exist in bd 1.x and must not appear in repo docs;
  - hook slowness caused by JSONL import is fixed by removing the import (and, for multi-clone repos, moving to a Dolt remote), never by bypassing or timing out hooks.
  A test asserts the heading exists and the section contains `bd dolt push`, `bd dolt pull` and no recommendation of `bd sync`, `BEADS_HOOK_TIMEOUT`, `--no-verify` or `core.hooksPath`.
- Both doctors warn, one line each, when the current beads repo (`.beads/` exists):
  - (a) has an effective post-checkout or post-merge hook, or bd config, that imports the JSONL on checkout. The warning names the section above.
  - (b) has AGENTS.md, CLAUDE.md or README.md at the repo root mentioning `bd sync`. The warning names the file and count.
  Each check has fixture tests in `tests/test_agent_env_doctor.py` and `tests/test_agent_env_doctor_codex.py`, including a clean fixture (no import, clean docs) that produces neither warning, and a non-beads repo that produces neither.
- No missing-remote warning at SessionStart: a repo that works only in linked worktrees legitimately has no remote. The section documents `bd dolt remote list` as the manual check instead. (If a later issue wants a doctor check, it must first establish how to tell a multi-clone repo from a single-clone one.)
- Proof at close: the close note names the new tests with failing-then-passing runs, and shows the doctor output on a checked-in fixture that reproduces shatter's pre-migration state (its beads post-checkout hook with the JSONL import and an AGENTS.md excerpt with `bd sync`): expected, both warnings. Real shatter output is optional and only if shatter has not migrated yet; closure must not depend on shatter staying broken.

## Suggested approach

- For check (a), first confirm from bd 1.1 docs and source how the checkout import is triggered and disabled (hook section vs config key); record the finding in the issue before writing the check; build one fixture per trigger.
- Check (a) reads files only; it runs no `bd` subprocess, so it adds no SessionStart latency.

## Out of scope

- The one-sentence rule in land-work and beads-issue-flow, untracking the export, and the tracked-export doctor warning (bento-49pg).
- Having land.py export and commit the JSONL (dropped per D4 and 49pg).
- Editing shatter's AGENTS.md, hooks or remote setup (shatter beads-retire-jsonl-import-dolt-remote).
- Any hook bypass or `BEADS_HOOK_TIMEOUT` guidance (D4).

## Priority / Type / Labels

P1 / task / audit, beads-issue-flow, hooks

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

Blocked by bento-49pg (via the note draft 49pg-dolt-remote-section-note): the section sits next to 49pg's sentence and must not contradict it. Blocks git-hook-latency-visibility.

---

<!-- 05-check-unpushed-overcount-and-blocks.md -->

---
slug: check-unpushed-overcount-and-blocks
kind: new
title: "check-unpushed Stop hook: count against all remotes (not @{u}), report without blocking on the primary branch in the primary checkout, and do not block a branch that a live land.py is landing"
priority: P2
type: bug
labels: [audit, hooks, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# check-unpushed Stop hook: count against all remotes (not @{u}), report without blocking on the primary branch in the primary checkout, and do not block a branch that a live land.py is landing

Complements bento-neng (closed), which covered only the identical re-nag. Related: bento-neng, bento-k23u (closed; subagents under a lead hold), bento-2p2p (closed 2026-09-23 as 3731a6f; decoupled the Beads exemptions in the same file), bento-x4bm (open; adds a durable landing record under `<git-common-dir>/bento/landing/<issue-id>/`), bento-i76i (open; blocks commit verbs on the primary branch). Source findings: bento-11, sessions-10 (shatter audit 2026-09-22).

## Problem

The Stop hook `check-unpushed.py` fires at the end of every turn and blocks on "unpushed" commits. In shatter it:

1. overcounts after a rebase, because it counts `@{u}..HEAD` and the old upstream lacks the main commits pulled in by the rebase;
2. blocks on the shared primary `main`, where peer sessions leave commits;
3. blocks every turn while this session's `land.py` is still running in the background.

That removes the agent's legitimate way to wait (ending its turn) and feeds busy-wait `sleep 1; echo` loops. One shatter session made 609 such calls.

## Evidence (shatter transcripts since 2026-09-04)

- 77 blocks in 21 sessions. Session 9f13ca23 had 31 blocks out of 95 stop events while land.py ran in the background ("Still mid-landing ... that hook will clear once this branch merges").
- Rebase overcounts: "'str-6vl7p-redundant-canonicalize' has 71 unpushed commits" (10 times), and counts of 68, 69, 72 and 115.
- Primary: "Session end blocked: branch 'main' has 2 unpushed commits" (17 times), and "main has uncommitted changes and 1 unpushed commit" (13 times).

## Current code facts (bento origin/main @ 0b8d488, after 3731a6f)

- `catalog/hooks/bento/claude/scripts/check-unpushed.py` lines 515-520: resolves `@{u}` and counts `git rev-list @{u}..HEAD --count`; line 540 lists `@{u}..HEAD`.
- Lines 591-625: the block messages ("Session end blocked: ..." and "Session end still blocked ...").
- Lines 232-257: the cooperative, session-scoped turn-hold marker (`_turn_hold_path`, `consume_turn_hold`), keyed on the payload `session_id`; line 695 onward: per-session throttle state (bento-neng).
- Git reflogs carry no agent session id, and `land.py` does not know the session id of the agent that started it. So "this session's commits" and "this session's land.py" cannot be attributed by session; the design below uses the branch and a live process instead.

## Design contract

- **Ownership is by branch, not session.** land.py writes one marker per branch it is landing: `<git-common-dir>/bento/landing-in-progress/<branch-slug>.json` with `{branch, worktree, pid, pid_start_time, host, started_at}`. `pid_start_time` is field 22 of `/proc/<pid>/stat` (or `ps -o lstart=` where /proc is absent). The marker is written atomically (temp file + `os.replace`) after `prepare` succeeds and removed in `finally`.
- **Liveness.** The hook treats a marker as live only if `host` matches, the pid exists, and its start time equals `pid_start_time`. A reused pid or dead process makes the marker stale; the hook ignores it for blocking, removes it, and mentions the stale marker once.
- **Scope of the suppression.** The hook suppresses the unpushed-commit block only when the Stop payload's worktree is on the marker's `branch` (or is the marker's `worktree`). Uncommitted changes still block as today. Concurrent landers of different branches have separate markers. The marker is created exclusively (`O_CREAT|O_EXCL` on the temp name, then a check that no live marker exists for the branch), so a second land.py for the same branch while the first is live fails fast with a clear message; a stale marker is replaced.
- **A bare background `git push`** started without land.py is not covered: the hook still blocks. The block message says so.
- If bento-x4bm's landing-record directory lands first, the marker may live beside it; the contract above still applies.

## Acceptance criteria

- The count uses `git rev-list HEAD --not --remotes`. Test: a fixture where a feature branch with 2 own commits is rebased onto an origin/main that gained 50 commits reports 2, not 52.
- In the primary checkout, on the primary branch, the hook prints the unpushed/uncommitted summary to stderr and exits 0 (never 2). Test included. The message notes that commits on the primary branch are blocked at commit time by the git guard (bento-i76i), so the Stop hook no longer duplicates that.
- Marker tests, each asserting the hook's exit code: live marker for the current branch (exit 0, no block); live marker for a different branch (still blocks); marker with a dead pid (blocks, marker removed); marker whose pid is alive but whose start time differs (blocks, marker removed); marker from another host (ignored); two concurrent landers of different branches (each only unblocks its own branch).
- land.py tests: the marker exists while a stubbed verifier sleeps, and is gone after both success and an injected failure; a second land.py for the same branch while the first is live fails with the documented message.
- The existing check-unpushed tests still pass, including the bento-2p2p exemption tests.
- Proof at close: the close note names the new tests with failing-then-passing runs.

## Out of scope

- Moving the check to SessionEnd (worth noting as a follow-up option).
- Attributing commits to agent sessions.

## Priority / Type / Labels

P2 / bug / audit, hooks, land-work

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None hard. Coordinate the marker location with bento-x4bm if it is in flight.

---

<!-- 06-doctor-state-per-worktree.md -->

---
slug: doctor-state-per-worktree
kind: new
title: "Resolve .agent-mode.local repo-wide (main working tree), with a locked writer, so linked-worktree sessions see collapsed doctor state and primary-checkout decisions"
priority: P2
type: bug
labels: [audit, hooks]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Resolve .agent-mode.local repo-wide (main working tree), with a locked writer, so linked-worktree sessions see collapsed doctor state and primary-checkout decisions

bento-rdtn.2 is closed but its goal is not met in linked worktrees (a note on rdtn.2 points here). Source findings: bento-08, plugins-08 (context), shatter audit 2026-09-22. Related, touching the same code: bento-m4y5 (in_progress; the agent-env doctor hook), bento-xy8m (open; `hook_bypass` reported as unknown). Coordinate with whichever is in flight.

## Problem

bento-rdtn.2 stores the doctor's seen/decided/remind_after keys in `.agent-mode.local` at `git rev-parse --show-toplevel`, which is per checkout. Launch-work requires work in linked worktrees, so every working session:

- gets the full dormancy paragraphs and the "prints once per repo" superpowers notice again;
- gets a fresh `.agent-mode.local` written into its worktree root;
- misses settings recorded in the primary checkout's copy, such as any `agent_env_doctor_skip_plugin`, `require_pushed=false`, `require_worktree=false` or `hook_bypass=allow` decision.

## Evidence (shatter)

- Primary `/home/ketan/project/shatter/.agent-mode.local`: `dangerous`, `agent_env_doctor_seen=bugshot,storystore`, `agent_env_doctor_superpowers_pointer_seen=true`.
- Linked worktree `audit-2026-09-22/.agent-mode.local` (created 2026-09-22 12:16): only the seen keys that session wrote.
- Running the doctor in the worktree prints the full text; in the primary it prints one-liners.

## Current code facts (bento origin/main @ 0b8d488; cited files unchanged since b1bb787)

- `catalog/hooks/bento/claude/scripts/agent-env-doctor.py` line 198: root resolved with `git -C <cwd> rev-parse --show-toplevel`; line 588: `config = root / ".agent-mode.local"`.
- Lines 1040-1088 `_rewrite_agent_mode_keys`: an **unlocked** read-modify-write that writes a fixed temp name `.agent-mode.local.tmp` and `replace`s it. Its docstring already names the race with a concurrent SessionStart. Today each checkout has its own file, so the race is mostly per checkout; moving every worktree onto one file makes it a real lost-update risk.
- Other readers with the same per-checkout scope: `require-worktree-git-guard.py` line 85 (`_read_agent_mode_keys`), `check-unpushed.py` lines 101-129 (`require_pushed`, `require_landed`), `require-worktree.sh`, `catalog/hooks/hygiene/claude/scripts/hygiene-check.py`, and the Codex copies of the doctor and check-unpushed.
- Outside bento: the dotfiles launcher `bashrc.agent-mode.sh` (`_agent_mode_repo_root`, line 29) reads the `dangerous` line from `--show-toplevel` too.

## Key scope table (the issue must keep this table in the skill/hook docs)

| Key | Scope after this issue |
|---|---|
| `agent_env_doctor_seen`, `agent_env_doctor_superpowers_pointer_seen`, `agent_env_doctor_skip_plugin`, decided/remind_after keys | repo-wide |
| `require_pushed`, `require_landed`, `require_worktree`, `hook_bypass` | repo-wide |
| `dangerous` / `mode=` (launcher) | unchanged; read by the dotfiles launcher, not by bento |

## Acceptance criteria

- One resolver, shared by the doctor (Claude and Codex), the git guard, `require-worktree.sh`, check-unpushed (both) and hygiene-check: when the absolute `git rev-parse --git-common-dir` ends in `/.git`, the file is `<its parent>/.agent-mode.local` (this holds even when the primary has `core.bare=true`, as shatter's does, so the existing primary copy keeps working); for any other common dir (a true bare repo), the file is `<git-common-dir>/bento/agent-mode.local`. Reads and writes both use it. Tests cover a normal repo, a linked worktree, a primary with `core.bare=true`, and a true bare repo with linked worktrees.
- **Locked writer.** `_rewrite_agent_mode_keys` takes an exclusive `fcntl.flock` on a sibling lock file for the whole read-modify-write, writes to a unique temp name (`tempfile.NamedTemporaryFile(dir=..., delete=False)`), and `os.replace`s it. Test: 8 processes started together, each adding a distinct key 20 times, end with all 8 keys present and no `.tmp` files left; the same test on the pre-fix writer loses keys (record that failing run).
- Behaviour tests: seen state written from one linked worktree collapses output in another linked worktree of the same repo; `hook_bypass=allow` and `require_pushed=false` set in the main copy are honoured in a linked worktree.
- **Migration preserves decisions.** On first run, an existing worktree-local `.agent-mode.local` is merged into the repo-wide file under the lock: keys missing from the repo-wide file are copied; on a conflicting value the repo-wide value wins and a one-line notice names the key and both values. Seen-lists (`agent_env_doctor_seen`) are unioned. The worktree-local file is then left in place but ignored, and the notice says so. Tests cover copy, conflict and union.
- Proof at close: the close note names the new tests with failing-then-passing runs, including the concurrent-writer test.

## Out of scope

- New doctor checks.
- The dotfiles launcher's `dangerous` lookup (a dotfiles change, if wanted).

## Priority / Type / Labels

P2 / bug / audit, hooks

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None hard. Coordinate with bento-m4y5 and bento-xy8m.

---

<!-- 07-doctor-state-reopen-note.md -->

---
slug: doctor-state-reopen-note
kind: reopen-note
title: "Note on closed bento-rdtn.2: doctor seen/decided state is per checkout, so linked worktrees get full nudges"
priority: P2
type: note
labels: [audit, hooks]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: [doctor-state-per-worktree]
existing_id: bento-rdtn.2
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-rdtn.2

Target: bento-rdtn.2 (closed). Action: add a comment. Do not reopen; the work is tracked in the new issue. Post after doctor-state-per-worktree is filed, and replace the slug with its id.

## Comment text

Audit 2026-09-22 (shatter; finding bento-08): the collapsed-state goal of this issue is not met in linked worktrees. The seen/decided/remind_after keys live in `.agent-mode.local` at `git rev-parse --show-toplevel` (agent-env-doctor.py line 198, line 588), which is per checkout. Launch-work puts every working session in a linked worktree, so each one gets the full dormancy and superpowers text again, writes a fresh `.agent-mode.local` into its worktree root, and misses decisions recorded in the primary checkout's copy (for example `hook_bypass=allow`). Example: shatter primary has `agent_env_doctor_seen=bugshot,storystore`, while the `audit-2026-09-22` worktree's copy holds only what that session wrote. Follow-up: <id of doctor-state-per-worktree>.

---

<!-- 08-stale-previews-leak-and-scoping.md -->

---
slug: stale-previews-leak-and-scoping
kind: new
title: "Stale land-work previews: bento test suite leaks previews into /tmp; preview creation ignores TMPDIR; doctor discovery is unscoped, /tmp-only, and names a closure mode that does not exist"
priority: P2
type: bug
labels: [audit, land-work, closure, hygiene]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Stale land-work previews: bento test suite leaks previews into /tmp; preview creation ignores TMPDIR; doctor discovery is unscoped, /tmp-only, and names a closure mode that does not exist

Related: bento-rdtn.1, bento-7n7, bento-d91, bento-e583 (open; preview owner lock between concurrent landers). Source findings: bento-09, sessions-17 (shatter audit 2026-09-22).

## Problem

1. Bento's own land-work tests leave preview worktrees in the shared `/tmp`.
2. `default_preview_dir()` hard-codes `/tmp`, so tests cannot isolate it through `TMPDIR`.
3. The doctor's stale-preview check globs every `/tmp/land-work-preview-*` from every repo and prints one line per directory. It looks only in `/tmp` (`main()` defaults `tmp_root` to `Path("/tmp")`), so once creation honours `TMPDIR`, previews created elsewhere would silently drop out of discovery.
4. The doctor says "let closure clean it up", but closure has no apply mode for previews.

## Evidence

- 2026-09-22: `/tmp` held 89-92 `land-work-preview-*` dirs, 86 created that day. Their `.git` files point at deleted `/tmp/tmpXXXX/repo` fixture repos, and their contents are bento fixture files (README.md, feature.txt, swarm-config.json, .land-work/).
- 2026-09-23 re-checks: 130, then 132 `land-work-preview-*` dirs, almost all newer than 2026-09-22. The leak continues.
- A simulated `collect_warnings` for the shatter worktree at now+2d produced 98 warnings, 89 of them "stale land-work preview".

## Current code facts (bento origin/main @ 0b8d488; cited files unchanged since b1bb787)

- `catalog/skills/land-work/scripts/land-work-create-preview.py` lines 84-85: `default_preview_dir()` returns `Path(tempfile.mkdtemp(prefix="land-work-preview-", dir="/tmp")).resolve()`.
- `catalog/hooks/bento/claude/scripts/agent-env-doctor.py` lines 791-812 (`check_stale_previews`): `tmp_root.glob("land-work-preview-*")`, no repo scoping, one warning per entry, wording "... remove it or let closure clean it up". Line 1191: `resolved_tmp_root = tmp_root if tmp_root is not None else Path("/tmp")`. The Codex doctor has its own copy.
- `catalog/skills/closure/scripts/closure-scan.py` line 1660: `--apply` choices are `[APPLY_DELETE_LOCAL_MERGED, APPLY_DELETE_LOCAL_PATCH_EQUIVALENT]` only.

## Acceptance criteria

- **Creation.** `default_preview_dir` uses `tempfile.mkdtemp(prefix="land-work-preview-")` with no `dir=`, so it honours `TMPDIR`.
- **Discovery agrees with creation.** Both doctors find previews in two ways and de-duplicate by real path:
  - owned previews: registered worktrees of the current repo (`git worktree list --porcelain`) whose directory name starts with `land-work-preview-`, wherever they live;
  - unowned previews: `land-work-preview-*` under both `tempfile.gettempdir()` and `/tmp` whose `.git` file points at a gitdir that no longer exists.
  Test: a stale preview of the current repo created under a non-`/tmp` `TMPDIR` is reported; one under `/tmp` still is.
- **Output.** Owned stale previews collapse to one line, "N stale land-work previews for this repo (oldest X days)". Unowned previews collapse to one separate line, "N orphaned land-work previews whose repo no longer exists", shown at most once per day (seen-state throttled). Previews of other live repos are not reported. Tests cover each.
- **Test hygiene.** Land-work tests set a per-test `TMPDIR`. A conftest session fixture records `/tmp/land-work-preview-*` before and after the session and fails if any were added.
- **No reference to a nonexistent mode.** Either closure gains `--apply remove-stale-previews` (dry-run by default; removes only previews with no live owner per bento-e583's lock, and only via `git worktree remove` for owned previews), or the doctor wording drops the closure reference and prints the exact removal commands. A test asserts the doctor never names an `--apply` mode that `closure-scan.py --help` does not list.
- Proof at close: the close note shows the leak-detection fixture failing on the pre-fix code and passing after, and a full land-work test run leaving zero new `/tmp/land-work-preview-*` entries (before/after counts).

## Out of scope

- Preview owner locking between concurrent landers (bento-e583).
- Removing the previews already leaked on the maintainer's machine (a one-off manual cleanup).

## Priority / Type / Labels

P2 / bug / audit, land-work, closure, hygiene

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None. If the closure apply mode is chosen, it must use bento-e583's owner lock once that lands.

---

<!-- 09-closure-orphan-worktree-dirs.md -->

---
slug: closure-orphan-worktree-dirs
kind: note-to-existing
title: "Note on bento-nljv: audit 2026-09-22 confirms the five shatter orphan dirs persist; add a build-output-only classification so non-empty orphans get a reviewable, per-path cleanup"
priority: P2
type: note
labels: [audit, closure, hygiene]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-nljv
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-nljv

Target: bento-nljv (open, P3, "closure: report orphan directories under the dedicated worktree root, and safely remove the empty ones"). Action: add a comment. Do not file a new issue.

Why this is a note and not a new issue: the audit draft asked for a closure apply mode that deletes orphan worktree directories. bento-nljv already specifies the orphan report, the shared orphan rule with bento-8oj0 (which also removes the doctor's wrong "safe to remove" wording), and a safe empty-only removal. The draft's deletion rule ("not a registered worktree and no process cwd inside") was unsafe: it would delete a slash-branch parent holding a live worktree (bento-8oj0), and it treated an unregistered directory as disposable even though it may hold the only copy of uncommitted work. nljv's rule, "non-empty orphans are never deleted automatically", is the correct one and is kept.

Source finding: bento-10 (shatter audit 2026-09-22).

## Comment text

Audit 2026-09-22 (shatter; finding bento-10), re-checked 2026-09-23:

- The five shatter orphans this issue cites are still present and the doctor has flagged them at every session start since the 2026-09-04 audit: `str-6q1i` (109M, a partial source checkout: `.beads/`, `Cargo.toml`, `PROTOCOL.md`, ...), `str-hszo-tmpfix` (573M), and `str-k6e61-scm-followups`, `str-mambd-enum-variant-gen`, `str-yhsp-concolic-run` (16K each). Shatter's AGENTS.md forbids agents from deleting worktree dirs, so nobody acts on the warning; about 700 MB has sat there since June to July 2026.
- Of these, the empty-only apply mode here would remove none of the space: all five are non-empty.

Suggested addition, keeping this issue's "never delete non-empty automatically" rule:

1. Add `content_class` to each `orphan_worktree_dirs` entry: `empty`, `build-output-only` (every file lies under a top-level `target/`, `node_modules/`, `dist/`, `build/` or `.venv/`), or `other`. For `other`, include up to 10 sample relative paths outside those directories so a reviewer can see what would be lost.
2. Keep deletion of non-empty orphans behind explicit per-path confirmation. If a mode is added (for example `--apply remove-build-output-orphans --path <p>`), it accepts only paths listed in the latest scan with `content_class: build-output-only`, reuses this issue's no-follow, snapshot and `registered-now` checks, and removes only the build-output subtrees plus the then-empty directory. `other` orphans are report-only, with "inspect before deleting" wording.
3. Tests: a `build-output-only` orphan is removed only when its path is passed; an `other` orphan (for example one with a modified tracked-looking file such as `src/lib.rs`) is never removed by any mode; a slash-branch parent containing a registered worktree at any depth is never an orphan (the shared fixture table with bento-8oj0).
4. Close-time proof for that addition: the scan JSON from the maintainer's machine showing the five shatter entries classified (expected: `str-6q1i` is `other`, the rest `build-output-only` if they hold only `target/`).

---

<!-- 10-close-reason-evidence.md -->

---
slug: close-reason-evidence
kind: new
title: "beads-issue-flow: close helper for closes that are not landings (not reproducible, duplicate, wontfix, superseded) with typed reason validation and SHA ancestry checks, plus a report of non-conforming close reasons"
priority: P2
type: feature
labels: [audit, beads-issue-flow]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: [wzbt-manual-close-note]
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# beads-issue-flow: close helper for closes that are not landings (not reproducible, duplicate, wontfix, superseded) with typed reason validation and SHA ancestry checks, plus a report of non-conforming close reasons

Source finding: prior-13 (shatter audit 2026-09-22). Related: bento-1qry, bento-1bl, bento-v57, bento-m4en.

**Ownership boundary with the closure group** (bento-wzbt, then bento-sy49 and bento-79j2, then bento-bo9c, then bento-x4bm):

- Closing an issue **after a landing** is owned by that group. bento-wzbt defines the closure-note contract and the close reason for landings; bento-x4bm makes `land.py` validate the note and close the issue itself. This issue does not touch `land.py` and does not define a landing reason format.
- This issue owns the **other** closes, which today have no contract at all: not reproducible, duplicate, wontfix, and superseded. It reuses wzbt's contract module or grammar file for anything shared (SHA and issue-id syntax) rather than defining a second one.

## Problem

Closures that are not landings carry no verifiable evidence, and one was checked against a commit that is not on main.

## Evidence (shatter)

- 17 of 49 closures since 2026-09-04 have the reason "Closed" or an empty reason, including str-qwua7.8, .9, .15, .27, .32, .41, .56 and str-2tyfk (empty).
- str-qwua7.14's reason is "Not reproducible against current main (e50fc399)". `git merge-base --is-ancestor e50fc399 origin/main` is false: e50fc399 is a stray fixture "init" commit.
- By contrast, land.py-era reasons (str-qwua7.4, str-vr7vq, str-gjsb2) carry the SHA, the gate and the review result.

## Current code facts (bento origin/main @ 0b8d488)

- `catalog/skills/beads-issue-flow/SKILL.md`: the ancestry rule (`git merge-base --is-ancestor <merge-sha> <integration-branch>`, line 138) and the Closure Checklist (line 174) are guidance only (bento-1bl, bento-v57, closed).
- `land.py` has no close step today (bento-x4bm adds it); `land-work-verify-landing.py` `_check_issue_status` (lines 77-130) only warns.
- `bd close` accepts any reason, including an empty one. Bento has no hook on `bd close`.

## Reason grammar (validated by type)

| Type | Form | Validation |
|---|---|---|
| not reproducible | `not reproducible at <sha> (<command>)` | `<sha>` is 7-40 hex and `git merge-base --is-ancestor <sha> <remote>/<primary>` succeeds; `<command>` is non-empty |
| duplicate | `duplicate of <id>` | `bd show <id>` succeeds and `<id>` differs from the issue being closed |
| superseded | `superseded by <id>[: <text>]` | as for duplicate |
| wontfix | `wontfix: <rationale>` | rationale has at least 3 words (so `wontfix: obsolete` is rejected and `wontfix: obsolete after str-81xiw removal` passes) |

Anything that matches none of these forms is rejected, whatever its length. The helper refuses a landing-style reason (`<sha> landed on ...`) and points to land-work, which owns those.

## Acceptance criteria

- A helper, `catalog/skills/beads-issue-flow/scripts/close.py <id> --reason "..."`, validates the reason against the table and then runs `bd close <id> --reason ...`. Ancestry and id-resolution failures refuse the close unless `--force --justification "<text>"` is given; the justification is appended to the reason.
- Tests, table-driven, cover for each type at least one valid reason and one invalid reason, including: `Closed`, an empty reason, 60 characters of free prose (rejected), `wontfix: obsolete` (rejected), `not reproducible at e50fc399 (...)` against a fixture where that SHA is not an ancestor (rejected), and the forced path.
- beads-issue-flow's Closure Checklist tells agents to use the helper for every close that is not a landing, and names bento-wzbt's contract for landings.
- **Enforcement is stated honestly.** The skill says the helper is the supported path and that a direct `bd close` bypasses it. To detect bypasses, `close.py --audit --since <date>` lists closed issues whose reason matches neither this grammar nor wzbt's landing form. A test covers the audit on a stubbed `bd` output.
- Proof at close: the close note names the tests with failing-then-passing runs; includes `close.py --audit --since 2026-09-04` output against shatter (expected: at least the 17 bare or empty reasons listed above); and this issue's own close reason follows wzbt's landing contract.

## Out of scope

- Closes that follow a landing, and `land.py` (bento-wzbt, bento-x4bm).
- A PreToolUse guard on `bd close` (possible follow-up on top of bento-l01v's segmenter).
- Retroactively fixing old close reasons.

## Priority / Type / Labels

P2 / feature / audit, beads-issue-flow

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

Blocked by bento-wzbt (the contract whose shared grammar this reuses), via the note draft wzbt-manual-close-note. Blocks followups-as-siblings (which extends the helper).

---

<!-- 11-claim-branch-reconciliation.md -->

---
slug: claim-branch-reconciliation
kind: new
title: "closure: report stale in_progress claims (no branch/worktree, or branch already merged into the primary branch)"
priority: P2
type: feature
labels: [audit, closure, hygiene]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# closure: report stale in_progress claims (no branch/worktree, or branch already merged into the primary branch)

Related: bento-rdtn.8, bento-rdtn.9 (closed), bento-x4bm (open). Source findings: prior-10, bento-17 (shatter audit 2026-09-22).

**Scope boundary.** Making a landing close its issue is owned by bento-x4bm (land.py validates the closure note, lands, then closes the issue and records the landing durably; it fails preflight if the issue is already closed). The audit draft's first criterion ("land.py closes the issue in the same run, or verify-landing exits non-zero") duplicated x4bm and is dropped. This issue covers only the detection side: claims that no live work backs, whatever path created them.

## Problem

Stale `in_progress` claims keep recurring in shatter, in both directions:

1. **Landed but not closed.** str-mpgg1 was merged on 2026-09-02 (84941b37 is an ancestor of origin/main) and is still in_progress (re-checked with `bd show str-mpgg1` on 2026-09-23). Of the 13 stale claims cleared by shatter str-qwua7.17 on 09-08, 8 had also landed without being closed.
2. **Claimed with no live work.** str-8q1b4's branch was 105 commits behind main with its last commit on 2026-08-31 when drift-patrol flagged it for the third time. (It has since landed and was closed on 2026-09-23; the pattern, not this instance, is the point.)

bento-rdtn.8 only warns at verify-landing. bento-rdtn.9 reports the opposite case: branches whose issue is not in_progress.

## Current code facts (bento origin/main @ 0b8d488; cited files unchanged since b1bb787)

- `catalog/skills/land-work/scripts/land-work-verify-landing.py` lines 77-130 (`_check_issue_status`): warns "`<id>` is not closed ..." and does not fail. `land.py` calls verify-landing without `--issue` (line 330).
- `catalog/skills/closure/scripts/closure-scan.py` lines 1539-1591: the rdtn.9 `tracker_mismatch` annotation, branch-to-issue direction only.

## Acceptance criteria

- `closure-scan.py` output gains `stale_claims`, listing in_progress issues in two classes:
  - `merged`: the issue's branch (matched by the existing branch-to-issue rule) is an ancestor of `<remote>/<primary>`;
  - `no_live_work`: claimed more than N days ago (default 7, `--stale-claim-days`), with no matching local branch, registered worktree or remote branch.
  Each entry has the issue id, claim age, class, and the evidence (branch name and merge-base result, or "no branch/worktree found"). Report only; no automatic release or close.
- The human-readable closure report prints the two classes with the exact `bd` command a person would run (`bd close <id> --reason ...` pointing at land-work's contract for `merged`, `bd update <id> --status open` for `no_live_work`), without running either.
- Tests use a fixture repo and a stubbed `bd` that returns in_progress issues covering: merged branch, unmerged live branch (not reported), no branch and older than N days (reported), no branch and newer than N days (not reported), and a `bd` failure (the section is skipped with a warning; the rest of the scan still succeeds).
- Proof at close: the close note names the tests with failing-then-passing runs, and includes the `stale_claims` section from a real run against shatter.

## Out of scope

- Automatically releasing or closing claims.
- Closing issues as part of landing (bento-x4bm).

## Priority / Type / Labels

P2 / feature / audit, closure, hygiene

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None.

---

<!-- 12-followups-as-siblings.md -->

---
slug: followups-as-siblings
kind: new
title: "Review follow-ups should be filed as siblings with discovered-from, not children of the issue being closed; the non-landing close helper refuses parents with open children"
priority: P3
type: feature
labels: [audit, beads-issue-flow, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: [close-reason-evidence]
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Review follow-ups should be filed as siblings with discovered-from, not children of the issue being closed; the non-landing close helper refuses parents with open children

Source finding: prior-11 (shatter audit 2026-09-22). Related: bento-x4bm (land.py closes issues after landing; a companion comment below asks it to apply the same open-children check), bento-wzbt (closure contract).

## Problem

Code-review follow-ups are filed as children of the issue that is about to be closed, so they become permanent orphans. Shatter drift-patrol lists four:

- str-qwua7.56.1 (parent str-qwua7.56 closed)
- str-qwua7.9.1 (parent str-qwua7.9 closed)
- str-hy9b.J3 and str-hy9b.1 (epic str-hy9b closed 2026-04-27; open for 5 months)

Shatter's epic-lifecycle rule issue, str-5b9f, has been open since June.

## Current code facts (bento origin/main @ 0b8d488)

- `catalog/skills/land-work/SKILL.md` line 227 ("Create tracker follow-up items for Minor issues ...") and `catalog/skills/beads-issue-flow/SKILL.md` lines 185-187 ("file a follow-up issue ...") do not say where follow-ups go.
- bd supports `--deps discovered-from:<id>` and `--parent`.

## Where the check lives

Closes happen on two supported paths, and each gets the check:

- after a landing: `land.py` (bento-x4bm). Requested through the companion comment below; not implemented by this issue.
- every other close: `close.py` from close-reason-evidence. Implemented here.

A direct `bd close` bypasses both; the `close.py --audit` report from close-reason-evidence is the detective control. This issue does not claim enforcement beyond that.

## Acceptance criteria

- beads-issue-flow and land-work say: file follow-ups as siblings under the same parent epic, or as top-level issues, with `discovered-from:<closing-id>`, never as children of the issue being closed. A grep test asserts the sentence appears in both skills.
- `close.py` (close-reason-evidence) refuses to close an issue that has open children unless `--force --justification "<text>"` is given, and lists the open children in the refusal.
- Tests with a stubbed `bd`: refusal with one open child, success with only closed children, and the forced path.
- `close.py --audit` also lists closed issues that still have open children (the drift-patrol case above).
- Proof at close: the close note names the tests with failing-then-passing runs, and includes `close.py --audit` output against shatter showing the four orphans above (or whichever remain open).

## Out of scope

- Re-parenting shatter's existing orphans (shatter tracker work).
- Implementing the check inside land.py (bento-x4bm; see companion comment).

## Priority / Type / Labels

P3 / feature / audit, beads-issue-flow, land-work

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

Blocked by close-reason-evidence (the close helper this extends). The doc change can land first.

## Comment for `bento-x4bm`

> Audit 2026-09-22 (shatter; finding prior-11): review follow-ups filed as children of the issue being closed become permanent orphans (shatter has four: str-qwua7.56.1, str-qwua7.9.1, str-hy9b.J3, str-hy9b.1). <id of followups-as-siblings> adds an open-children refusal to the non-landing close helper. Please give land.py's tracker step the same check: at `issue_preflight`, fail if the issue has open children (list them), so a landing never closes a parent over open children. Suggested test: a stubbed `bd` returning one open child makes preflight fail before `prepare`.

---

<!-- 13-per-subagent-scratch-dirs.md -->

---
slug: per-subagent-scratch-dirs
kind: new
title: "code-bloat-sniffer and swarm prompts: give each parallel subagent or teammate its own scratch subdirectory and forbid rm -rf outside it"
priority: P3
type: task
labels: [audit, swarm, skills]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# code-bloat-sniffer and swarm prompts: give each parallel subagent or teammate its own scratch subdirectory and forbid rm -rf outside it

Related: bento-btv (closed; added hygiene rules to the swarm worker prompt, nothing about scratch dirs). Source finding: cli-ux-20 (shatter audit 2026-09-22).

**Scope correction from review.** The audit draft targeted "the audit skill's subagent prompt" and "`proj/` example paths". Neither exists in bento: `catalog/skills/audit/` (SKILL.md, references/, scripts/audit-discover.py) has no subagent prompt and no parallel dispatch, and `git grep 'proj/' -- catalog/skills` finds nothing. The prompt that caused the incident came from the shatter audit workflow script, which is not a bento template. The bento surfaces that do dispatch parallel agents from a prompt template are listed below; this issue covers only those.

## Problem

Parallel subagents inherit the parent session's scratchpad path, so they share one directory. During the shatter audit on 2026-09-22, one reviewer replaced `scratchpad/proj` while another was using it: a later `cp` failed with "cannot stat .../proj/01-arithmetic.ts". An earlier `rm -rf scratchpad/proj` by one reviewer may have deleted another reviewer's files.

Bento templates that fan out work in parallel carry no rule about scratch space:

- `catalog/skills/code-bloat-sniffer/SKILL.md` step 4 (lines 40-54) dispatches one subagent per chunk and lists what each receives; nothing about where it may write. Its tool guide (`references/tools-by-language.md` line 54) tells subagents to "copy `go.sum`/`go.mod` to a scratch dir", so parallel Go chunks can collide on the same scratch path.
- `catalog/skills/swarm/SKILL.md` lines 236-245: the working-hygiene rules every teammate prompt must carry (also required by `CODEX.md` line 42). Teammates get their own worktrees, but nothing covers temp or scratch files outside the worktree.

## Acceptance criteria

- code-bloat-sniffer step 4 adds to what each subagent receives: "write temporary files only under `<scratch>/<chunk-slug>/`, created at start; never `rm -rf` outside it". The Go `go mod tidy` instruction in `tools-by-language.md` uses that per-chunk directory.
- swarm's teammate working-hygiene rules add the same rule, keyed by the teammate's issue id (`<scratch>/<issue-id>/`), so it lands in every teammate prompt.
- A test (in the existing skill-lint or catalog tests) asserts both skills contain the rule text, and fails if either is removed.
- Proof at close: the close note names the test with a failing-then-passing run.

## Out of scope

- Claude Code harness scratchpad behaviour.
- The shatter audit workflow script itself (a shatter or dotfiles concern).
- The audit skill, which dispatches no subagents today.

## Priority / Type / Labels

P3 / task / audit, swarm, skills

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None.

---

<!-- 14-a0nz-behavioural-probe.md -->

---
slug: a0nz-behavioural-probe
kind: note-to-existing
title: "Note on bento-a0nz: audit grades need claim-appropriate evidence (production reachability for wiring claims, default-configuration probes for behaviour claims); add a Rust reachability mechanism; build the CLI from the audited SHA"
priority: P3
type: note
labels: [audit, skills]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-a0nz
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-a0nz

Target: bento-a0nz (open, P1, "Add invocation/wiring + entrypoint-parity detection to the audit skill"). Action: add a comment; not a new issue. Source finding: core-23 (shatter audit 2026-09-22).

## Comment text

Addendum (shatter audit 2026-09-22, finding core-23).

In the prior shatter audit, grades rested on reading code, and three findings were wrong:

- it graded dead `clustering.rs` "Solid" (a wiring claim: the module had no production caller);
- it asserted solver timeouts that are off by default (a behaviour claim: the code exists but the default configuration never enables it);
- it filed str-qwua7.49 on a wrong premise.

Additional requirement for the audit skill: the evidence for each graded component must match the kind of claim.

- **Wiring or integration claims** ("X is used", "X is solid in production") need a production-reachability citation: a caller chain from a real entry point (CLI command, handler, exported API), not a test. A behavioural probe does not satisfy this, because a probe can exercise code that nothing in production reaches.
- **Behaviour claims** ("X times out", "X rejects Y") need a probe or E2E run **in the default configuration**, recording the command and the configuration used. A caller citation does not satisfy this, because it cannot show that a feature is enabled by default.

Where to put it: there is no executable report validator in the audit skill today (`catalog/skills/audit/` has prose instructions in SKILL.md and `references/quality-standards.md`, plus `scripts/audit-discover.py`). There is also no component-grade report template. Add this as a **review checklist item** in `references/quality-standards.md` and in the part of SKILL.md that describes the report's findings, rather than claiming anything "rejects" a grade. Building a validator is out of scope for this addendum.

Rust gap: `catalog/skills/audit/references/static-analysis-tools.md` lists reachability tools for Go (deadcode) and TS (knip), but its Rust section has only clippy, fmt, cargo-audit and tarpaulin. Add a Rust mechanism, for example a module-reachability script (pub modules with zero references outside their own tests) or cargo-udeps plus a pub-item grep.

Also: before filing behavioural findings, build the CLI from the audited SHA and record `--version` and the SHA. Shatter's str-qwua7.12 was filed from a stale binary.

Proof for these additions at close: (1) the checklist text in `quality-standards.md` and SKILL.md, with a grep test that it is present; (2) the Rust reachability mechanism run on a fixture crate with one dead pub module and one live one, flagging only the dead one.

---

<!-- 15-cross-check-stop-hook-hijack.md -->

---
slug: cross-check-stop-hook-hijack
kind: new
title: "cross-check: bento Stop hook hijacks the read-only Codex reviewer's final message; identity check then fails, or passes on an empty review"
priority: P1
type: bug
labels: [audit, hooks, cross-check]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# cross-check: bento Stop hook hijacks the read-only Codex reviewer's final message; identity check then fails, or passes on an empty review

Found while cross-checking the shatter 2026-09-22 audit issue drafts (2026-09-23).

## Problem

`cross-check-run.py` runs the counterpart as `codex exec --sandbox read-only ... -o <last-message-file> -`
with `cwd` set to the caller's working directory, and exports `CROSS_CHECK_ACTIVE=1`. Codex has bento
installed (`~/.codex/plugins/cache/bento/bento/2.3.73`, `[features] hooks = true`), so bento's
`check-unpushed.py` Stop hook fires inside the reviewer. None of the bento hook scripts read
`CROSS_CHECK_ACTIVE` (the constant exists only in `skills/cross-check/scripts/cross_check_common.py`).

When the reviewed repo's worktree has uncommitted changes, the hook blocks the reviewer's Stop. Codex
then answers the hook, and that answer becomes the final message the `-o` file captures. It is not
the review. Two failure modes follow:

1. **False fallback.** The hook reply has no identity block, so `validate_identity` fails ("reviewer
   omitted the required identity block"), the script exits 4, and the caller falls back to a
   same-runtime DEGRADED review. The full Codex review in the discarded output is lost: on failure the
   runner deletes `last_file` and prints `output=<none written>`.
2. **False success.** When Codex copies the identity block into its hook reply, validation passes and
   a "cross" review is written whose whole body is the hook reply. Observed text: "The review made no
   edits. The stop hook is reporting existing branch changes; committing those would exceed the
   read-only review scope…".

## Evidence

- In a 22-bucket run on 2026-09-23 (the shatter audit worktree had uncommitted draft files), 16 runs
  exited 4 on identity validation and 1 "passed" with a hook-reply body
  (`shatter/audits/2026-09-22/issues/crosscheck/shatter-frontend-rust.md`, branch `audit-2026-09-22`).
- The same bundle type, rerun with a clean worktree, produced a valid review. Codex stderr shows
  `hook: Stop` / `hook: Stop Completed` right after the identity block.
- `grep -rn CROSS_CHECK_ACTIVE plugins/*/bento/hooks` returns nothing.
- `cross-check-run.py` `finally:` unlinks `last_file` on every path, including identity failure.

## Acceptance criteria

- Every bento hook that can block (at least Stop/`check-unpushed.py`, and any PreToolUse deny) exits 0
  without output when `CROSS_CHECK_ACTIVE=1`, in both the Claude and Codex plugin trees. A test covers
  each hook.
- `validate_identity` (or the runner) rejects a review whose body is a hook reply or has no findings
  structure. At minimum, a body with no severity-tagged finding and no "ready to file" verdict counts
  as a failure. There is a regression test using the observed hook-reply text.
- On identity failure, the runner keeps the raw counterpart output next to the fallback message
  (e.g. `/tmp/cross-check-<slug>-<ts>.rejected.md`) so an expensive review is never silently lost.
- End-to-end proof: run `cross-check-run.py --artifact-type issue` against a dirty worktree with Codex
  installed. It exits 0 with a real review. Record the output in the close reason.

## Suggested approach

Add an early `if recursion_active(os.environ): sys.exit(0)` to the hook entry points (a shared helper
next to `bento_telemetry.py`). Consider also running the counterpart with `cwd` set to a neutral temp
directory and passing repo paths in the scope, which is the workaround used on 2026-09-23.

## Out of scope

Changing the identity-block protocol itself.

---

<!-- 16-l01v-residual-bypasses-note.md -->

---
slug: l01v-residual-bypasses-note
kind: note-to-existing
title: "Note on bento-l01v: residual guard bypasses (/usr/bin/git, timeout/nice/ionice/sudo, -C, earlier cd, GIT_CONFIG_* env) are tracked separately and build on shell_segments.py"
priority: P1
type: note
labels: [audit, hooks, safety]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-l01v
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-l01v

Target: bento-l01v (open, P2, "git guard: shared shell segmenter ..."). Action: add a comment. Do not change its scope. This draft exists so the filer adds the dependency edge git-guard-bypasses-and-false-positives blocked-by bento-l01v.

## Comment text

Audit 2026-09-22 (shatter; findings sessions-02, bento-04). The guard's most common real-world bypass is not in this issue's scope: in shatter session c1689435 (2026-09-21/22), after the guard shipped, about 22 commits and 18 pushes were spelled `/usr/bin/git -c core.hooksPath=/dev/null commit|push ...`, and shatter's memory tells agents to prefer `/usr/bin/git`. That, the `timeout`/`nice`/`ionice`/`sudo` wrappers, `-C <path>` and earlier-`cd` repo resolution (this issue's stated known limit), and hook disabling through `GIT_CONFIG_COUNT/KEY/VALUE` or `GIT_CONFIG_PARAMETERS` are tracked in <id of git-guard-bypasses-and-false-positives>, which is blocked by this issue and builds on `shell_segments.py` instead of adding a second tokenizer.

One API request for this issue so the follow-up does not need to change the corpus: have `shell_segments` expose the `VAR=value` assignments it strips (for example a `command_segments_with_env()` sibling returning `(env, argv)` per segment), since the follow-up must inspect `GIT_CONFIG_*` assignments.

Please add the follow-up to the shared-parser landing order after bento-i76i (and bento-nfi3). Because that P1 follow-up is blocked here, consider raising this issue to P1.

---

<!-- 17-i76i-switch-update-ref-note.md -->

---
slug: i76i-switch-update-ref-note
kind: note-to-existing
title: "Note on bento-i76i: also deny `git switch <primary>` and `git update-ref refs/heads/<primary>` in the primary checkout"
priority: P2
type: note
labels: [audit, hooks, safety]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-i76i
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-i76i

Target: bento-i76i (open, P2, "git guard: block commit, cherry-pick, pull, am, and revert on the primary branch ..."). Action: add a comment. This issue owns the primary-branch verb list, so the two verbs the audit found go here rather than into a separate issue. The draft also gives the filer the ordering edge git-guard-bypasses-and-false-positives blocked-by bento-i76i (same file, shared landing order).

## Comment text

Audit 2026-09-22 (shatter; finding bento-04). Two more primary-branch mutations pass the guard today (probed at bento 0b8d488 with cwd = the primary checkout; both exit 0):

- `git switch main` is the `switch` spelling of the already-denied `checkout <primary>`; deny it under the same conditions (and `switch -C`/`--force-create <primary>`).
- `git update-ref refs/heads/main <sha>` moves the primary branch directly, skipping every other rule; deny `update-ref` whose ref is `refs/heads/<primary>` (including `-d`) in the primary checkout.

Suggested tests: both commands exit 2 in the primary checkout, and `git switch feature` / `git update-ref refs/heads/feature <sha>` exit 0. The remaining bypass forms (`/usr/bin/git`, wrappers, `-C`, `cd`, `GIT_CONFIG_*` env) are tracked in <id of git-guard-bypasses-and-false-positives>, which lands after this issue.

---

<!-- 18-49pg-dolt-remote-section-note.md -->

---
slug: 49pg-dolt-remote-section-note
kind: note-to-existing
title: "Note on bento-49pg: a follow-up adds the beads-issue-flow \"Snapshot and Dolt remote\" section your one-sentence rule can point to"
priority: P2
type: note
labels: [audit, beads-issue-flow, hooks]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-49pg
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-49pg

Target: bento-49pg (open, P2, "Untrack the Beads JSONL export ..."). Action: add a comment. Scope unchanged. This draft gives the filer the edge beads-dolt-remote-guidance blocked-by bento-49pg.

## Comment text

Audit 2026-09-22 (shatter; finding bento-16; maintainer decision D4). Shatter's bd post-checkout hook spends about 6 minutes importing `.beads/issues.jsonl` on every checkout, worktree creation and landing preview, and its AGENTS.md still requires `bd sync`, which bd 1.1.0 does not have. <id of beads-dolt-remote-guidance> adds a `## Snapshot and Dolt remote` section to beads-issue-flow (no JSONL import on checkout; linked worktrees share one database per `bd worktree --help`; Dolt remote only for separate clones or machines; no `bd sync`) and two doctor checks (import-on-checkout, `bd sync` in repo docs). It is blocked by this issue and keeps this issue's one-sentence rule and `check_tracked_beads_export` as the single source for untracking; it only adds the longer section and links to it.

---

<!-- 19-wzbt-manual-close-note.md -->

---
slug: wzbt-manual-close-note
kind: note-to-existing
title: "Note on bento-wzbt: a follow-up adds a close helper for non-landing closes (not reproducible, duplicate, superseded, wontfix) that reuses this contract's shared syntax"
priority: P2
type: note
labels: [audit, beads-issue-flow]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-wzbt
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-wzbt

Target: bento-wzbt (open, P2, "land-work: define the closure-note contract ..."). Action: add a comment. Scope unchanged. This draft gives the filer the edge close-reason-evidence blocked-by bento-wzbt.

## Comment text

Audit 2026-09-22 (shatter; finding prior-13). Closes that are not landings have no contract: in shatter, 17 of 49 closures since 2026-09-04 carry the reason "Closed" or nothing, and str-qwua7.14 was closed "Not reproducible against current main (e50fc399)" where e50fc399 is not an ancestor of origin/main. <id of close-reason-evidence> adds a beads-issue-flow close helper for those closes only (not reproducible at an ancestor SHA, duplicate of, superseded by, wontfix with a rationale), plus an audit report of non-conforming close reasons. It is blocked by this issue so that it reuses this contract's SHA and issue-id syntax and names this contract as the rule for landings; it does not touch land.py (bento-x4bm) or define a landing reason. Please mention the non-landing forms in `closure-contract.md` as "see beads-issue-flow close helper", so the two stay one system.
