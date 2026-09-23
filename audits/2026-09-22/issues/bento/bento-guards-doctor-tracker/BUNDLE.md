# Issue bundle: bento-guards-doctor-tracker

- Audit: shatter audit 2026-09-22
- Bucket: bento-guards-doctor-tracker
- Repo: bento (tracker: bd in /home/ketan/project/bento, prefix bento)
- Parent epic: Epic: Audit 2026-09-22 findings (bento)
- Theme: guards, doctor and tracker flow (git-guard bypasses, git-hook latency visibility, beads Dolt-remote guidance, check-unpushed, doctor state, previews, closure, close evidence)
- Code facts re-verified against bento origin/main @ b1bb787 (2026-09-23).

## Maintainer decisions (2026-09-23)

- D1 Releases: keep Windows (x86_64-pc-windows-msvc) and aarch64-unknown-linux-gnu in the release matrix; fix them (Z3 header/static link on Windows; openssl-sys under cross for aarch64). Release work closes only with a green release-run URL.
- D2 shatter diff: retire the snapshot-diff command and the unused Snapshot writer path; spec-diff is the regression tool. Update SPEC/README/QUICKSTART. The `diff` name becomes free; str-81xiw decides its reuse. Correct shatter-agents' `shatter diff --staged` docs.
- D3 Concolic positioning: measure first. P1 controlled default-vs-concolic benchmark; P1 fix concolic early termination; a follow-up decision issue re-decides "concolic-first" positioning. No doc softening now.
- D4 Beads hook stall: bd's post-checkout hook spends ~6 min importing .beads/issues.jsonl (waiting, not computing); the JSONL is an export, not sync. Retire the JSONL import in shatter; move tracker sync to a Dolt remote; verify first whether stale imports clobbered newer DB state; AGENTS.md drops `bd sync`; str-qwua7.28 superseded; bento beads-issue-flow gets matching guidance. No BEADS_HOOK_TIMEOUT fix and no hook-bypass guidance.
- D5 Git identity: leaked [user] section already removed. Draft a .mailmap, a git-state check (drift-patrol/setup-hooks), and fixture .git/config snapshotting.
- D6 Filing: after reconciliation and Codex cross-check the maintainer runs one filer script. Nothing is filed by agents.

D4 shapes this bucket: git-hook-latency-visibility (warn and point, no bypass) and beads-dolt-remote-guidance (P1).

## Contents

- 01-git-guard-bypasses-and-false-positives.md — P1 — new — Git guard: close /usr/bin/git, wrapper-prefix, -C, cd and GIT_CONFIG bypasses; stop false positives on quoted/heredoc text
- 02-git-guard-reopen-note.md — P1 — reopen-note on bento-rdtn.15 — Note on closed bento-rdtn.15: guard is bypassed by /usr/bin/git, wrapper prefixes, -C, cd and GIT_CONFIG env
- 03-git-hook-latency-visibility.md — P1 — new — launch-work and land-work: time git subprocesses and name slow hooks; guard's slow-hook pointer must cite real guidance; doctor flags stale beads hook markers
- 04-beads-dolt-remote-guidance.md — P1 — new — beads-issue-flow: issues.jsonl is an export, not sync; no JSONL import on checkout; cross-machine sync via Dolt remote; doctor checks import-on-checkout, missing remote and bd sync mentions
- 05-check-unpushed-overcount-and-blocks.md — P2 — new — check-unpushed Stop hook: count against all remotes (not @{u}), skip checkouts the session did not modify, and do not block while this session's land.py is running
- 06-doctor-state-per-worktree.md — P2 — new — agent-env-doctor and require-worktree hooks read .agent-mode.local per checkout, so linked-worktree sessions never see collapsed state or primary-checkout decisions
- 07-doctor-state-reopen-note.md — P2 — reopen-note on bento-rdtn.2 — Note on closed bento-rdtn.2: doctor seen/decided state is per checkout, so linked worktrees get full nudges
- 08-stale-previews-leak-and-scoping.md — P2 — new — Stale land-work previews: bento test suite leaks previews into /tmp; default_preview_dir ignores TMPDIR; doctor warning is unscoped and names a closure mode that does not exist
- 09-closure-orphan-worktree-dirs.md — P2 — new — closure: add an apply mode for orphan worktree directories the doctor flags as "safe to remove"
- 10-close-reason-evidence.md — P2 — new — Close flow: reject bare "Closed" reasons and verify cited SHAs are ancestors of the primary branch
- 11-claim-branch-reconciliation.md — P2 — new — Reconcile claims with branches: fail verify-landing when the landed issue stays in_progress; report in_progress issues with no branch/worktree
- 12-followups-as-siblings.md — P3 — new — Review follow-ups should be filed as siblings with discovered-from, not children of the issue being closed; guard closing parents with open children
- 13-per-subagent-scratch-dirs.md — P3 — new — swarm/audit prompt templates: give each parallel subagent its own scratch subdirectory
- 14-a0nz-behavioural-probe.md — P3 — note-to-existing on bento-a0nz — Note on bento-a0nz: audit grades need a production-caller check or behavioural probe; add a Rust reachability mechanism; build the CLI from the audited SHA

---

<!-- 01-git-guard-bypasses-and-false-positives.md -->

---
slug: git-guard-bypasses-and-false-positives
kind: new
title: "Git guard: close /usr/bin/git, wrapper-prefix, -C, cd and GIT_CONFIG bypasses; stop false positives on quoted/heredoc text"
priority: P1
type: bug
labels: [audit, hooks, safety]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Git guard: close /usr/bin/git, wrapper-prefix, -C, cd and GIT_CONFIG bypasses; stop false positives on quoted/heredoc text

Follow-up to closed bento-rdtn.15, whose goal is not met. Source findings: sessions-02, bento-04, sessions-15 (shatter audit 2026-09-22). Related: bento-rdtn.15 (a reopen note points here).

## Problem

bento-rdtn.15 added `require-worktree-git-guard.py`, a PreToolUse Bash hook. It blocks branch-mutating git in the primary checkout and blocks hook bypasses (`--no-verify`, `-c core.hooksPath=`). The guard only inspects a command segment whose first non-`VAR=value` token is literally `git`, so:

- **Bypasses (false negatives).** The spellings agents use most are not inspected. In shatter session c1689435 (2026-09-21 23:03 to 09-22 02:00), after the guard shipped, about 22 commits and 18 pushes were spelled `/usr/bin/git -c core.hooksPath=/dev/null commit|push ...`. Shatter's memory tells agents to use `/usr/bin/git`, and the global RTK guidance tells them to prefix commands with `rtk`.
- **False positives.** `_SEGMENT_SPLIT_RE` splits on `;`, `|`, `&` and newlines even inside quotes and heredoc bodies. A read-only `python3 - <<'EOF' ... re.search(r'...|git merge|...') ... EOF` run from the primary checkout was blocked with "Blocked: 'git merge' mutates the checkout in the primary checkout".

## Evidence

Synthetic PreToolUse payloads piped into the guard (exit 2 = blocked).

Blocked (exit 2), as intended:
- `git commit --no-verify -m x`
- `git -c core.hooksPath=/dev/null push`
- `git merge feature` (cwd = primary checkout)

Allowed (exit 0), but should be blocked:
- `/usr/bin/git commit --no-verify -m x`
- `/usr/bin/git -c core.hooksPath=/dev/null commit -m x`
- `timeout 60 git push --no-verify`
- `rtk git commit --no-verify -m x`
- `env git commit --no-verify`
- `git -C <primary> merge feature` (cwd elsewhere)
- `cd <primary> && git merge feature`
- `GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0=/dev/null git commit`
- `git update-ref refs/heads/main <sha>`
- `git switch main` (in the primary checkout)

False positive: a heredoc or quoted string containing `git merge` is blocked.

## Current code facts (bento origin/main @ b1bb787, re-verified 2026-09-23)

- Source: `catalog/hooks/bento/claude/scripts/require-worktree-git-guard.py`. The generated copy is `plugins/claude/bento/hooks/scripts/require-worktree-git-guard.py`; do not edit it by hand, rebuild it with `scripts/build-plugins`.
- Line 44: `_SEGMENT_SPLIT_RE = re.compile(r"&&|\|\||[;&|\n]")`.
- Lines 132-146: `_find_git_segments` skips leading `VAR=value` tokens without inspecting them, then requires `tokens[idx] == "git"`.
- The repo comes from the payload cwd. The path after `-C` is skipped, not resolved, and an earlier `cd` in the same chain is ignored.
- Line 207: any command containing `LAND_WORK_MARKER` (`BENTO_LAND_WORK=1`, line 42) is exempted wholesale. Keep that, but document it.
- Lines 229-236: the hook-bypass block message points to "the launch-work skill's dependency-bootstrap guidance for slow-hook fixes". That pointer is fixed by git-hook-latency-visibility, not here.

## Acceptance criteria

- Each "allowed but should be blocked" command above exits 2, with the existing message style.
- A heredoc or quoted string containing `git merge`, `--no-verify` or `core.hooksPath` is not blocked when no git process is invoked. At least three negative cases: a python heredoc, a `grep 'git merge'` pattern, and an `echo "... --no-verify ..."`.
- A table-driven test covers every case above, and the existing cases still pass.
- Proof at close: the close note names the new test file and records a failing run of the new cases before the fix and a passing run after, plus a passing `scripts/build-plugins` check that the generated copy matches the source.

## Suggested approach

- Tokenize with quote and heredoc awareness: strip heredoc bodies before splitting, then split with `shlex.shlex(punctuation_chars=True)`.
- Treat `os.path.basename(tok) == "git"` as git.
- Skip wrapper commands and their arguments before the git token: `timeout N`, `nice [-n N]`, `ionice ...`, `env [VAR=..]`, `command`, `rtk`, `sudo`.
- Resolve the effective repo from `-C <path>` and from the last `cd <path>` earlier in the chain.
- Inspect `GIT_CONFIG_KEY_n`/`GIT_CONFIG_VALUE_n` and `GIT_CONFIG_PARAMETERS` for `core.hooksPath`.
- Add `switch <primary-branch>` and `update-ref refs/heads/<primary>` to the mutation list.

## Out of scope

- Changing which operations are policy-forbidden.
- The block message's slow-hook pointer (git-hook-latency-visibility).
- The per-checkout scope of the `hook_bypass=allow` opt-out (doctor-state-per-worktree).

## Priority / Type / Labels

P1 / bug / audit, hooks, safety

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None.

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
- `rtk git commit --no-verify`, `timeout 60 git push --no-verify`, `env git ...`
- `git -C <primary> merge ...` and `cd <primary> && git merge ...`
- `GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0=/dev/null git commit`

In shatter session c1689435 (2026-09-21/22), after this guard shipped, about 22 commits and 18 pushes used the `/usr/bin/git -c core.hooksPath=/dev/null` form. The guard also blocks read-only commands whose quoted or heredoc text contains `git merge`. Follow-up: <id of git-guard-bypasses-and-false-positives>.

---

<!-- 03-git-hook-latency-visibility.md -->

---
slug: git-hook-latency-visibility
kind: new
title: "launch-work and land-work: time git subprocesses and name slow hooks; guard's slow-hook pointer must cite real guidance; doctor flags stale beads hook markers"
priority: P1
type: bug
labels: [audit, land-work, launch-work, hooks]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: [beads-dolt-remote-guidance]
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# launch-work and land-work: time git subprocesses and name slow hooks; guard's slow-hook pointer must cite real guidance; doctor flags stale beads hook markers

Source findings: bento-03 (also sessions-05), shatter audit 2026-09-22. Maintainer decision D4 (2026-09-23) applies. Related shatter issue: beads-retire-jsonl-import-dolt-remote (the shatter-side root-cause fix; mention in the body only, it is not a bento id).

## Problem

In shatter, git hooks installed by beads stall every worktree creation and every landing for minutes. Bento scripts that create worktrees pay that cost silently and in series (`launch-work-bootstrap --apply`, `land-work-create-preview.py`, merge/push in `land.py`). Agents see a hung command with no explanation and reach for hook bypasses. The bento git guard blocks the bypass, but its message sends them to "the launch-work skill's dependency-bootstrap guidance for slow-hook fixes", and that reference has no hook content.

The root cause was measured on 2026-09-23 (D4): bd's post-checkout hook spends about 6 minutes "importing JSONL from .beads/issues.jsonl" (1,773 issues, about 10 s of CPU, so it is waiting, not computing). bd itself warns that the JSONL "is an export, not cross-machine sync or source of truth" and suggests a Dolt remote. The fix for that is the shatter issue beads-retire-jsonl-import-dolt-remote, plus the bento guidance in beads-dolt-remote-guidance. This issue makes the cost visible in bento's tools and points agents at the right fix. It deliberately adds no hook bypass and no timeout override.

## Evidence (shatter, 2026-09-19..23)

- `land.py` `create_preview` took 234.9-301.2 s in 11 successful landings (one 10.9 s outlier on 09-21).
- Every `launch-work-bootstrap --apply` exceeded the Bash tool timeout: more than 120 s on 09-19, more than 600 s on 09-21, more than 120 s for this audit's launch.
- Transcripts contain "beads: hook 'post-checkout' timed out after 300s" 32 times across 7 sessions.
- The installed hook `$(git rev-parse --git-common-dir)/hooks/post-checkout` in shatter carries `# --- BEGIN BEADS INTEGRATION v0.63.3 ---`, and `.beads/hooks/post-checkout` carries `# bd-hooks-version: 0.56.1`. `bd version` reports 1.1.0 (checked 2026-09-23).
- `time bd hooks run post-checkout` in a shatter linked worktree took 2m59.7s.

## Current code facts (bento origin/main @ b1bb787, re-verified 2026-09-23)

- `catalog/skills/launch-work/scripts/launch-work-bootstrap.py` lines 301-314: runs `git worktree add -b ...` with no timing or warning.
- `catalog/skills/land-work/scripts/land-work-create-preview.py` line 343: `git("worktree", "add", "--detach", ...)` for the preview, untimed.
- `catalog/hooks/bento/claude/scripts/require-worktree-git-guard.py` lines 229-236: the block message cites "the launch-work skill's dependency-bootstrap guidance for slow-hook fixes".
- `catalog/skills/launch-work/references/dependency-bootstrap.md`: zero matches for "hook" or "slow".
- `catalog/hooks/bento/claude/scripts/agent-env-doctor.py`: has a hook-binaries check (`check_hook_binaries`, line 389) but no hook-latency or beads hook-version check.

## Acceptance criteria

- `launch-work-bootstrap` and `land-work-create-preview.py` (and the merge/push subprocesses in `land.py`) time each git subprocess. When one takes longer than 30 s, they print a one-line warning naming the command and the hook(s) that ran, if they can tell (for example, a beads-managed `post-checkout`), and pointing to the beads-issue-flow Dolt-remote section. Tests use a fake slow hook in a fixture repo and assert the warning; a fast hook produces no warning.
- The guard's hook-bypass block message cites a section that exists and discusses slow hooks: the beads-issue-flow "Snapshot and Dolt remote" guidance from beads-dolt-remote-guidance. A test asserts the cited file and heading exist in the catalog.
- The doctor reports when a repo's beads hook markers (`BEADS INTEGRATION vX` in the common-dir hooks, `bd-hooks-version: X` in `.beads/hooks`) are older than `bd version`, and names `bd hooks install` as the refresh step. Tested with a fixture hook file.
- Nothing added by this issue sets `BEADS_HOOK_TIMEOUT`, `core.hooksPath`, `--no-verify` or any other hook bypass or timeout override, and no guidance recommends one. Reviewers check this in the diff.
- Proof at close: the close note names the new tests with failing-then-passing runs, and includes the output of one real `launch-work-bootstrap --apply` in a repo with a slow hook showing the warning.

## Suggested approach

- Add a small `timed_git(...)` helper in the shared script utilities that records wall time and, over the threshold, lists hooks present for that git operation (post-checkout for `worktree add`, post-merge for `merge`, pre-push for `push`) from the effective hooks path.
- Warn only; do not change hook behaviour for previews or work worktrees.

## Out of scope

- Any hook bypass, timeout override, or hydration suppression (D4).
- Shatter's own hook and JSONL import changes (shatter beads-retire-jsonl-import-dolt-remote).
- Fixing bd hook latency upstream.
- The guard's bypass detection (git-guard-bypasses-and-false-positives).

## Priority / Type / Labels

P1 / bug / audit, land-work, launch-work, hooks

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

Blocked by beads-dolt-remote-guidance: the guard pointer and the warning must cite the section that issue adds. The timing and doctor parts can start earlier.

---

<!-- 04-beads-dolt-remote-guidance.md -->

---
slug: beads-dolt-remote-guidance
kind: new
title: "beads-issue-flow: issues.jsonl is an export, not sync; no JSONL import on checkout; cross-machine sync via Dolt remote; doctor checks import-on-checkout, missing remote and bd sync mentions"
priority: P1
type: task
labels: [audit, beads-issue-flow, hooks]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# beads-issue-flow: issues.jsonl is an export, not sync; no JSONL import on checkout; cross-machine sync via Dolt remote; doctor checks import-on-checkout, missing remote and bd sync mentions

Source findings: bento-16, prior-03 (context), shatter audit 2026-09-22. Maintainer decision D4 (2026-09-23) applies. Raised to P1 because git-hook-latency-visibility points agents at the guidance this issue adds. Related: bento-rdtn.12 (closed; documented the bd CLI surface but not export or remote setup). Related shatter issue: beads-retire-jsonl-import-dolt-remote (mention in the body only).

## Problem

bento gives consuming repos no guidance on how beads state moves between checkouts and machines under bd 1.x, and repos have filled the gap with patterns that bd no longer supports:

- **JSONL import on checkout.** In shatter, bd's post-checkout hook spends about 6 minutes "importing JSONL from .beads/issues.jsonl" (1,773 issues, about 10 s CPU) on every worktree creation, checkout and landing preview (measured 2026-09-23, D4). bd itself warns that the JSONL "is an export, not cross-machine sync or source of truth" and suggests `bd dolt remote add origin ... && bd dolt push`. Importing a stale snapshot may also overwrite newer database state (shatter is verifying that in beads-retire-jsonl-import-dolt-remote).
- **`bd sync`.** bd 1.1.0 has no `bd sync`, but consumer docs still require it. Shatter's AGENTS.md mentions it 10 times, including a landing step.
- **No Dolt remote.** Hooks print "post-checkout JSONL import warning: no Dolt remote configured" (26 times in shatter transcripts).

Consequences in shatter: the tracked `.beads/issues.jsonl` has been frozen since 2026-09-07 (commit 134dd616), with 1,733 issues against 1,775 live and about 19 status mismatches, while CI drift-patrol and a branch-cleanup script read it; and one session hand-rolled a "bd sync" commit and pushed it straight to main with `--no-verify` on 09-06.

## Evidence (re-verified 2026-09-23)

- `bd sync --help` gives `Error: unknown command "sync" for "bd"` (bd 1.1.0).
- `bd dolt --help` lists `bd dolt remote add <name> <url>`, `bd dolt remote list`, `bd dolt push` and `bd dolt pull`.
- `grep -c 'bd sync' AGENTS.md` in shatter: 10.
- `git log -1 -- .beads/issues.jsonl` in shatter: 134dd616, 2026-09-07.

## Current code facts (bento origin/main @ b1bb787)

- `catalog/skills/beads-issue-flow/SKILL.md` has no JSONL-export or Dolt-remote section. Related text: line 19 ("Never read `.beads/`, `issues.jsonl`, or the Dolt ...") and lines 233-234 ("tracker-sync commits (e.g. the Beads export snapshot ...)").
- `catalog/skills/land-work/SKILL.md` lines 785-788: "Beads' `.beads/issues.jsonl` is a passive Dolt export and may be intentionally untracked ... do not re-add or commit it during landing."
- `catalog/hooks/bento/claude/scripts/agent-env-doctor.py` has no beads import, remote or `bd sync` check.

## Acceptance criteria

- beads-issue-flow gains a "Snapshot and Dolt remote" section (the heading git-hook-latency-visibility will cite) that states:
  - `.beads/issues.jsonl` is an export, not sync or source of truth; agents and scripts must not treat it as current tracker state;
  - repos should not import the JSONL on checkout (no JSONL import in post-checkout/post-merge hooks), and how to check whether a repo does;
  - cross-machine and cross-checkout sync uses a Dolt remote: `bd dolt remote list`, `bd dolt remote add origin <url>`, `bd dolt push`, `bd dolt pull`;
  - `bd sync` does not exist in bd 1.x and must not appear in repo docs;
  - hook slowness caused by JSONL import is fixed by removing the import and moving to a Dolt remote, never by bypassing or timing out hooks.
- beads-issue-flow lines 233-234 and land-work lines 785-788 are made consistent with that section.
- The doctor warns, one line each, when the current repo (a) has a beads hook or config that imports JSONL on checkout, (b) has no Dolt remote configured (`bd dolt remote list` is empty), or (c) has docs (AGENTS.md, CLAUDE.md, README.md) that mention `bd sync`. Each check has a fixture test, and a repo with a remote, no import and clean docs gets no warning.
- Proof at close: the close note names the new tests with failing-then-passing runs, and includes the doctor output for shatter before its migration (expected: all three warnings).

## Suggested approach

- For check (a), confirm with bd 1.1 docs and source how the checkout import is triggered and disabled (hook section vs config key) before writing the check; test against a fixture that mimics each.
- Run `bd dolt remote list` with a short timeout, and skip check (b) with a notice if bd is unavailable.

## Out of scope

- Having land.py export and commit the JSONL (dropped per D4).
- Editing shatter's AGENTS.md, hooks or remote setup (shatter beads-retire-jsonl-import-dolt-remote).
- Any hook bypass or `BEADS_HOOK_TIMEOUT` guidance (D4).

## Priority / Type / Labels

P1 / task / audit, beads-issue-flow, hooks

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None. Blocks git-hook-latency-visibility.

---

<!-- 05-check-unpushed-overcount-and-blocks.md -->

---
slug: check-unpushed-overcount-and-blocks
kind: new
title: "check-unpushed Stop hook: count against all remotes (not @{u}), skip checkouts the session did not modify, and do not block while this session's land.py is running"
priority: P2
type: bug
labels: [audit, hooks, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# check-unpushed Stop hook: count against all remotes (not @{u}), skip checkouts the session did not modify, and do not block while this session's land.py is running

Complements bento-neng (closed), which covered only the identical re-nag. Related: bento-neng, bento-k23u. Source findings: bento-11, sessions-10 (shatter audit 2026-09-22).

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

## Current code facts (bento origin/main @ b1bb787, re-verified 2026-09-23)

- `catalog/hooks/bento/claude/scripts/check-unpushed.py` lines 406-411: resolves `@{u}` and counts `git rev-list @{u}..HEAD --count`.
- Lines 476-510: the block messages ("Session end blocked: ... Note: Stop fires at the end of every turn ..." and "Session end still blocked ...").
- Line 581 onward: per-session throttle state (bento-neng).
- bento-k23u (closed) fixed blocking subagents under a lead hold.

## Acceptance criteria

- The count uses `git rev-list HEAD --not --remotes`. A rebased branch whose rebased-in commits are already on origin/main reports only its own commits. Test: a fixture with a rebase.
- In the primary checkout, on the primary branch, the hook reports without blocking unless the reflog shows this session created the commits. Test included.
- While a background `land.py` (or `git push`) started by this session is still running, detected via a marker/pid file that land.py writes, the hook does not block. Test included.
- The existing check-unpushed tests still pass.
- Proof at close: the close note names the new tests with failing-then-passing runs.

## Suggested approach

land.py writes `.land-work/in-progress.json` (pid, branch, session id, start time) in the git common dir and removes it in `finally`. The hook checks whether that pid is alive.

## Out of scope

- Moving the check to SessionEnd (worth noting as a follow-up option).

## Priority / Type / Labels

P2 / bug / audit, hooks, land-work

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None.

---

<!-- 06-doctor-state-per-worktree.md -->

---
slug: doctor-state-per-worktree
kind: new
title: "agent-env-doctor and require-worktree hooks read .agent-mode.local per checkout, so linked-worktree sessions never see collapsed state or primary-checkout decisions"
priority: P2
type: bug
labels: [audit, hooks]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# agent-env-doctor and require-worktree hooks read .agent-mode.local per checkout, so linked-worktree sessions never see collapsed state or primary-checkout decisions

bento-rdtn.2 is closed but its goal is not met in linked worktrees (a note on rdtn.2 points here). Source findings: bento-08, plugins-08 (context), shatter audit 2026-09-22.

## Problem

bento-rdtn.2 stores the doctor's seen/decided/remind_after keys in `.agent-mode.local` at `git rev-parse --show-toplevel`, which is per checkout. Launch-work requires work in linked worktrees, so every working session:

- gets the full dormancy paragraphs and the "prints once per repo" superpowers notice again;
- gets a fresh `.agent-mode.local` written into its worktree root;
- misses settings recorded in the primary checkout's copy, such as the `dangerous` launcher line and any `agent_env_doctor_skip_plugin`, `require_pushed=false` or `hook_bypass=allow` decision.

## Evidence (shatter)

- Primary `/home/ketan/project/shatter/.agent-mode.local`: `dangerous`, `agent_env_doctor_seen=bugshot,storystore`, `agent_env_doctor_superpowers_pointer_seen=true`.
- Linked worktree `audit-2026-09-22/.agent-mode.local` (created 2026-09-22 12:16): only the seen keys that session wrote.
- Running the doctor in the worktree prints the full text; in the primary it prints one-liners.

## Current code facts (bento origin/main @ b1bb787, re-verified 2026-09-23)

- `catalog/hooks/bento/claude/scripts/agent-env-doctor.py` line 198: root resolved with `git -C <cwd> rev-parse --show-toplevel`; line 588: `config = root / ".agent-mode.local"`; seen/decided writes use the same root.
- `catalog/hooks/bento/claude/scripts/require-worktree-git-guard.py` line 85: `_read_agent_mode_keys(repo_root)` has the same per-checkout scope. `check-unpushed.py` reads `require_pushed` the same way.

## Acceptance criteria

- The doctor, the require-worktree hooks and check-unpushed resolve `.agent-mode.local` from the main working tree (via `git rev-parse --git-common-dir`), for both reads and writes.
- Test: seen state written from one linked worktree collapses output in another linked worktree of the same repo; a `hook_bypass=allow` set in the primary is honoured in a linked worktree.
- Migration: an existing worktree-local `.agent-mode.local` is merged into the common one, or ignored with a one-line notice. Test included.
- Proof at close: the close note names the new tests with failing-then-passing runs.

## Out of scope

- New doctor checks.

## Priority / Type / Labels

P2 / bug / audit, hooks

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None.

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
title: "Stale land-work previews: bento test suite leaks previews into /tmp; default_preview_dir ignores TMPDIR; doctor warning is unscoped and names a closure mode that does not exist"
priority: P2
type: bug
labels: [audit, land-work, closure, hygiene]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Stale land-work previews: bento test suite leaks previews into /tmp; default_preview_dir ignores TMPDIR; doctor warning is unscoped and names a closure mode that does not exist

Related: bento-rdtn.1, bento-7n7, bento-d91, bento-e583. Source findings: bento-09, sessions-17 (shatter audit 2026-09-22).

## Problem

1. Bento's own land-work tests leave preview worktrees in the shared `/tmp`.
2. `default_preview_dir()` hard-codes `/tmp`, so tests cannot isolate it through `TMPDIR`.
3. The doctor's stale-preview check globs every `/tmp/land-work-preview-*` from every repo and prints one line per directory.
4. The doctor says "let closure clean it up", but closure has no apply mode for previews.

## Evidence

- 2026-09-22: `/tmp` held 89-92 `land-work-preview-*` dirs, 86 created that day. Their `.git` files point at deleted `/tmp/tmpXXXX/repo` fixture repos, and their contents are bento fixture files (README.md, feature.txt, swarm-config.json, .land-work/).
- 2026-09-23 re-check: `ls -d /tmp/land-work-preview-* | wc -l` gives 130, of which 127 are newer than 2026-09-22. The leak continues.
- A simulated `collect_warnings` for the shatter worktree at now+2d produced 98 warnings, 89 of them "stale land-work preview".

## Current code facts (bento origin/main @ b1bb787, re-verified 2026-09-23)

- `catalog/skills/land-work/scripts/land-work-create-preview.py` lines 84-85: `default_preview_dir()` returns `Path(tempfile.mkdtemp(prefix="land-work-preview-", dir="/tmp")).resolve()`.
- `catalog/hooks/bento/claude/scripts/agent-env-doctor.py` lines 791-812 (`check_stale_previews`): `tmp_root.glob("land-work-preview-*")`, no repo scoping, one warning per entry, wording "... remove it or let closure clean it up".
- `catalog/skills/closure/scripts/closure-scan.py` line 1660: `--apply` choices are `[APPLY_DELETE_LOCAL_MERGED, APPLY_DELETE_LOCAL_PATCH_EQUIVALENT]` only.

## Acceptance criteria

- `default_preview_dir` honours `TMPDIR` (`dir=None`, tempfile's default).
- Land-work tests set a per-test TMPDIR. A teardown assertion (or a conftest session fixture) fails if a test run created any `/tmp/land-work-preview-*`.
- The doctor reports only previews whose gitdir belongs to the current repo, collapsed to one line: "N stale land-work previews (oldest X days)".
- Either closure gains `--apply remove-stale-previews` (dry-run by default; skips previews with a live owner lock, see bento-e583), or the doctor wording drops the closure reference and gives the exact removal command.
- Proof at close: the close note shows the leak-detection fixture failing on the pre-fix code and passing after, and a full land-work test run leaving zero new `/tmp/land-work-preview-*` entries (before/after counts).

## Out of scope

- Preview owner locking between concurrent landers (bento-e583).
- Removing the previews already leaked on the maintainer's machine (a one-off manual cleanup).

## Priority / Type / Labels

P2 / bug / audit, land-work, closure, hygiene

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None.

---

<!-- 09-closure-orphan-worktree-dirs.md -->

---
slug: closure-orphan-worktree-dirs
kind: new
title: 'closure: add an apply mode for orphan worktree directories the doctor flags as "safe to remove"'
priority: P2
type: feature
labels: [audit, closure, hygiene]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# closure: add an apply mode for orphan worktree directories the doctor flags as "safe to remove"

Related: bento-rdtn.1. Source finding: bento-10 (shatter audit 2026-09-22).

## Problem

bento-rdtn.1 made the doctor detect worktree directories that are no longer registered git worktrees, and explicitly left removal to closure. No closure mode was added. In shatter the doctor has flagged the same five directories every session since the 2026-09-04 audit, and they are still there:

- `~/.local/share/worktrees/shatter/str-6q1i`
- `~/.local/share/worktrees/shatter/str-hszo-tmpfix`
- `~/.local/share/worktrees/shatter/str-k6e61-scm-followups`
- `~/.local/share/worktrees/shatter/str-mambd-enum-variant-gen`
- `~/.local/share/worktrees/shatter/str-yhsp-concolic-run`

Together they hold about 700 MB (109M, 573M and 3 x 16K), dated June to July 2026. Shatter's AGENTS.md forbids agents from deleting worktree dirs themselves, so nobody acts on the warning.

## Current code facts (bento origin/main @ b1bb787, re-verified 2026-09-23)

- `catalog/hooks/bento/claude/scripts/agent-env-doctor.py` lines 851-887 (`check_worktree_root_orphans`): orphan detection with the message "... worktree — dead directory left behind, safe to remove".
- `catalog/skills/closure/scripts/closure-scan.py` line 1660: the only apply modes are delete-local-merged and delete-local-patch-equivalent.

## Acceptance criteria

- `closure --apply remove-orphan-worktree-dirs`:
  - is dry-run by default, listing each path with its size and newest mtime;
  - on confirmation, removes only directories that are not registered worktrees of any repo and have no process with cwd inside them;
  - has tests covering a registered worktree (kept), an orphan (removed), and an orphan with a live process cwd inside (kept).
- The doctor message names this exact command.
- Proof at close: the close note names the tests with failing-then-passing runs and includes a dry-run listing from a real repo.

## Out of scope

- Changing shatter's AGENTS.md rule (shatter str-qwua7.23).

## Priority / Type / Labels

P2 / feature / audit, closure, hygiene

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None.

---

<!-- 10-close-reason-evidence.md -->

---
slug: close-reason-evidence
kind: new
title: 'Close flow: reject bare "Closed" reasons and verify cited SHAs are ancestors of the primary branch'
priority: P2
type: feature
labels: [audit, beads-issue-flow, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Close flow: reject bare "Closed" reasons and verify cited SHAs are ancestors of the primary branch

Related: bento-1qry, bento-1bl, bento-v57, bento-m4en. Source finding: prior-13 (shatter audit 2026-09-22).

## Problem

Closures outside the land.py path often carry no verifiable evidence. In one case a "not reproducible" closure was checked against a commit that is not on main.

## Evidence (shatter)

- 17 of 49 closures since 2026-09-04 have the reason "Closed" or an empty reason, including str-qwua7.8, .9, .15, .27, .32, .41, .56 and str-2tyfk (empty).
- str-qwua7.14's reason is "Not reproducible against current main (e50fc399)". `git merge-base --is-ancestor e50fc399 origin/main` is false: e50fc399 is a stray fixture "init" commit.
- By contrast, land.py-era reasons (str-qwua7.4, str-vr7vq, str-gjsb2) carry the SHA, the gate and the review result.

## Current code facts (bento origin/main @ b1bb787)

- `catalog/skills/beads-issue-flow/SKILL.md`: the ancestry rule (`git merge-base --is-ancestor <merge-sha> <integration-branch>`, line 138) and the Closure Checklist (line 174) are guidance only (bento-1bl, bento-v57, closed). Nothing enforces them.
- bento-1qry (in_progress) requires gate evidence in the land.py close note only.
- `bd close` accepts any reason, including an empty one.

## Acceptance criteria

- beads-issue-flow's Closure Checklist requires one of these reason forms:
  - `<sha> landed on <primary> (gate: ...)`
  - `not reproducible at <sha> (<command>)`
  - `duplicate of <id>`
  - `wontfix: <reason>`
- A helper script (for example `catalog/skills/beads-issue-flow/scripts/close.py <id> --reason ...`) rejects reasons shorter than 20 characters or equal to "Closed". For any 7-40 hex SHA in the reason, it runs `git merge-base --is-ancestor <sha> <remote>/<primary>` and refuses on failure unless `--force` is given with a justification.
- land.py's close step uses the helper.
- Tests cover every rejection case and the accepted forms.
- Proof at close: the close note names the tests with failing-then-passing runs, and this issue's own close reason is produced through the new helper.

## Out of scope

- Retroactively fixing old close reasons.

## Priority / Type / Labels

P2 / feature / audit, beads-issue-flow, land-work

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None. Blocks followups-as-siblings (which extends the helper).

---

<!-- 11-claim-branch-reconciliation.md -->

---
slug: claim-branch-reconciliation
kind: new
title: "Reconcile claims with branches: fail verify-landing when the landed issue stays in_progress; report in_progress issues with no branch/worktree"
priority: P2
type: feature
labels: [audit, closure, land-work, hygiene]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Reconcile claims with branches: fail verify-landing when the landed issue stays in_progress; report in_progress issues with no branch/worktree

Related: bento-rdtn.8, bento-rdtn.9. Source findings: prior-10, bento-17 (shatter audit 2026-09-22).

## Problem

Stale `in_progress` claims keep recurring in shatter, in both directions:

1. **Landed but not closed.** str-mpgg1 was merged on 2026-09-02 (84941b37 is an ancestor of origin/main) and is still in_progress (re-checked with `bd show str-mpgg1` on 2026-09-23). Of the 13 stale claims cleared by shatter str-qwua7.17 on 09-08, 8 had also landed without being closed.
2. **Claimed with no live work.** str-8q1b4's branch was 105 commits behind main with its last commit on 2026-08-31 when drift-patrol flagged it for the third time. (It has since landed and was closed on 2026-09-23; the pattern, not this instance, is the point.)

bento-rdtn.8 only warns at verify-landing. bento-rdtn.9 reports the opposite case: branches whose issue is not in_progress.

## Current code facts (bento origin/main @ b1bb787, re-verified 2026-09-23)

- `catalog/skills/land-work/scripts/land-work-verify-landing.py` lines 77-129 (`_check_issue_status`): appends a warning "`<id>` is not closed (status: ...); run: bd close ..." and does not fail.
- `catalog/skills/closure/scripts/closure-scan.py` lines 1539-1591: the rdtn.9 `tracker_mismatch` annotation only.

## Acceptance criteria

- land.py closes the tracker issue in the same run that pushes the primary branch, or verify-landing exits non-zero with an actionable message when the landed branch's issue is still open or in_progress.
- A closure report lists in_progress issues older than N days (default 7) with no matching local branch, worktree or remote branch, and in_progress issues whose named branch is already merged into the primary branch. Report only; no automatic release.
- Tests use a beads fixture or a stubbed bd.
- Proof at close: the close note names the tests with failing-then-passing runs and includes the report's output against shatter.

## Out of scope

- Automatically releasing claims.

## Priority / Type / Labels

P2 / feature / audit, closure, land-work, hygiene

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

None.

---

<!-- 12-followups-as-siblings.md -->

---
slug: followups-as-siblings
kind: new
title: "Review follow-ups should be filed as siblings with discovered-from, not children of the issue being closed; guard closing parents with open children"
priority: P3
type: feature
labels: [audit, beads-issue-flow, land-work]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: [close-reason-evidence]
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Review follow-ups should be filed as siblings with discovered-from, not children of the issue being closed; guard closing parents with open children

Source finding: prior-11 (shatter audit 2026-09-22).

## Problem

Code-review follow-ups are filed as children of the issue that is about to be closed, so they become permanent orphans. Shatter drift-patrol lists four:

- str-qwua7.56.1 (parent str-qwua7.56 closed)
- str-qwua7.9.1 (parent str-qwua7.9 closed)
- str-hy9b.J3 and str-hy9b.1 (epic str-hy9b closed 2026-04-27; open for 5 months)

Shatter's epic-lifecycle rule issue, str-5b9f, has been open since June.

## Current code facts (bento origin/main @ b1bb787)

- `catalog/skills/land-work/SKILL.md` line 227 ("Create tracker follow-up items for Minor issues ...") and `catalog/skills/beads-issue-flow/SKILL.md` lines 185-187 ("file a follow-up issue ...") do not say where follow-ups go.
- bd supports `--deps discovered-from:<id>` and `--parent`.

## Acceptance criteria

- beads-issue-flow and land-work say: file follow-ups as siblings under the same parent epic, or as top-level issues, with `discovered-from:<closing-id>`, never as children of the issue being closed.
- The close helper from close-reason-evidence refuses to close an issue that has open children unless `--force` is given with a reason.
- A test covers the refusal and the forced path.
- Proof at close: the close note names the test with a failing-then-passing run.

## Out of scope

- Re-parenting shatter's existing orphans (shatter tracker work).

## Priority / Type / Labels

P3 / feature / audit, beads-issue-flow, land-work

## Parent epic

Epic: Audit 2026-09-22 findings (bento)

## Dependencies

Blocked by close-reason-evidence (the close helper this extends). The doc change can land first.

---

<!-- 13-per-subagent-scratch-dirs.md -->

---
slug: per-subagent-scratch-dirs
kind: new
title: "swarm/audit prompt templates: give each parallel subagent its own scratch subdirectory"
priority: P3
type: task
labels: [audit, swarm, skills]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# swarm/audit prompt templates: give each parallel subagent its own scratch subdirectory

Related: bento-btv. Source finding: cli-ux-20 (shatter audit 2026-09-22). The verifier noted the prompt that caused the incident came from the shatter audit workflow script; this issue is scoped to bento's own swarm and audit templates, which should carry the rule so generated workflows inherit it.

## Problem

Parallel subagents inherit the parent session's scratchpad path, so they share one directory. During the shatter audit on 2026-09-22, one reviewer replaced `scratchpad/proj` while another was using it: a later `cp` failed with "cannot stat .../proj/01-arithmetic.ts". An earlier `rm -rf scratchpad/proj` by one reviewer may have deleted another reviewer's files. Prompts that use generic names such as `proj/` make collisions likely.

## Current code facts (bento origin/main @ b1bb787)

- bento-btv (closed) added hygiene rules to the swarm worker prompt but said nothing about scratch dirs.
- The swarm skill is `catalog/skills/swarm/` (SKILL.md, references/, scripts/); the audit skill is `catalog/skills/audit/` (SKILL.md, references/, scripts/).

## Acceptance criteria

- The swarm worker prompt template and the audit skill's subagent prompt tell each subagent to write only inside `<scratchpad>/<agent-or-area-name>/`, created at start.
- Both say not to `rm -rf` outside that subdirectory.
- Example paths in both skills use area-specific names, not `proj/`.
- A grep-based test asserts the rule appears in both templates.
- Proof at close: the close note names the test with a failing-then-passing run.

## Out of scope

- Claude Code harness scratchpad behaviour.
- The shatter audit workflow script itself.

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
title: "Note on bento-a0nz: audit grades need a production-caller check or behavioural probe; add a Rust reachability mechanism; build the CLI from the audited SHA"
priority: P3
type: note
labels: [audit, skills]
parent_epic: "Epic: Audit 2026-09-22 findings (bento)"
blocked_by: []
existing_id: bento-a0nz
tracker: "bd in /home/ketan/project/bento (prefix bento)"
---

# Note on bento-a0nz

Target: bento-a0nz (open). Action: add a comment; not a new issue. Source finding: core-23 (shatter audit 2026-09-22).

## Comment text

Addendum (shatter audit 2026-09-22, finding core-23).

Additional requirement for the audit skill: every graded component must cite either a production-caller check or one behavioural probe or E2E test. In the prior shatter audit, grades rested on reading code:

- it graded dead `clustering.rs` "Solid";
- it asserted solver timeouts that are off by default;
- it filed str-qwua7.49 on a wrong premise.

Rust gap: `catalog/skills/audit/references/static-analysis-tools.md` lists reachability tools for Go (deadcode) and TS (knip), but its Rust section has only clippy, fmt, cargo-audit and tarpaulin. Add a Rust mechanism, for example a module-reachability script (pub modules with zero references outside their own tests) or cargo-udeps plus a pub-item grep.

Also: before filing behavioural findings, build the CLI from the audited SHA and record `--version` and the SHA. Shatter's str-qwua7.12 was filed from a stale binary.

Proof for these additions at close: a test or fixture run showing the audit report template rejects a grade without a cited caller/probe, and the Rust reachability mechanism flagging a known-dead module in a fixture.
