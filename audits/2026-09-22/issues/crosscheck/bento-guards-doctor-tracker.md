# Cross-check review

- **Reviewer:** claude (DEGRADED same-runtime fallback)
- **Artifact type:** issue
- **Mode:** degraded

> **DEGRADED REVIEW.** The counterpart runtime was unavailable, so this review came from an independent agent of the *same* runtime. It shares the original author's model and blind spots; weight it accordingly.

## Findings

# Cross-check review (DEGRADED: same-runtime fallback): bento-guards-doctor-tracker

Reviewer: Claude, same runtime, read-only. The Codex counterpart failed identity validation (exit 4), so this is not an independent cross-runtime review.

Claims were checked against bento origin/main @ 5f8b850. That is 4 commits past the drafts' b1bb787, and none of those commits touch the files the drafts cite. Tracker checks used `bd show` and `bd list` in /home/ketan/project/bento, plus the shatter audit worktree.

## What verified

- Guard facts match the source: `_SEGMENT_SPLIT_RE` is at line 44, `_find_git_segments` at 132-146 requires `tokens[idx] == "git"`, `-C <path>` is skipped (not resolved) at lines 101-130, `LAND_WORK_MARKER` is exempted at line 207, and the block message at 229-236 cites the dependency-bootstrap guidance, which has 0 matches for hook or slow. The mutation list is `{merge, rebase, reset, clean}` plus checkout-primary, `branch -D` and force-push; it has no `switch` and no `update-ref`.
- Doctor facts match: `show-toplevel` at line 198, `.agent-mode.local` at 588, `check_hook_binaries` at 389, `check_stale_previews` unscoped glob with the "let closure clean it up" wording, and `check_worktree_root_orphans` with "safe to remove".
- `check-unpushed.py` lines 406-411 use `@{u}..HEAD`. closure-scan line 1660 offers only the two branch-delete apply modes. `default_preview_dir` uses `dir="/tmp"` (lines 84-85). The preview `worktree add` is at line 343. verify-landing `_check_issue_status` only warns.
- Shatter evidence matches: AGENTS.md has 10 `bd sync` mentions, and `.beads/issues.jsonl` was last committed in 134dd616 on 2026-09-07. `/tmp` now holds 132 `land-work-preview-*` dirs. The five orphan dirs have the stated sizes (109M, 573M, 3 x 16K) and none is a registered worktree.
- Every referenced bento issue has the stated status: rdtn.15, rdtn.2, rdtn.1, rdtn.8, rdtn.9, rdtn.12, neng, k23u and btv are closed; a0nz and e583 are open; 1qry is in_progress.

## Findings

### BLOCKER: 01 git-guard-bypasses-and-false-positives largely duplicates open bento-l01v, and partly bento-i76i (both filed 2026-09-23)
bento-l01v already specifies a shared `shell_segments.py` segmenter that does not match inside heredocs, quotes or comments and sees through `rtk`, `command` and `env` wrappers. That is the whole false-positive half of 01 plus 3 of its bypass cases. bento-i76i extends the primary-branch mutation list and fixes clustered `-n`. 01 proposes a different tokenizer (`shlex(punctuation_chars=True)` with heredoc stripping), so filing it as-is would create a competing implementation. Rescope 01 to the forms neither issue covers: `/usr/bin/git` (basename match), the `timeout`, `nice`, `ionice` and `sudo` wrappers, `-C <path>` and earlier-`cd` repo resolution, `GIT_CONFIG_COUNT/KEY/VALUE` and `GIT_CONFIG_PARAMETERS`, and `switch` and `update-ref` on the primary branch. It should be blocked by l01v, build on `shell_segments.py`, and join l01v's stated landing order (l01v, then 1srw, then i76i, then nfi3).

### MAJOR: 01 contradicts its own out-of-scope line
The out-of-scope section excludes "Changing which operations are policy-forbidden". The suggested approach and acceptance criteria then add `git switch main` and `git update-ref refs/heads/main` to the blocked set, which does change policy. Either treat these as a deliberate policy extension and coordinate them with bento-i76i, which owns primary-branch verbs, or drop them.

### MAJOR: 04 beads-dolt-remote-guidance overlaps open bento-49pg
bento-49pg records an owner decision (option C, 2026-09-22): untrack the JSONL export, sync through Dolt, reconcile the land-work "Tracker Handoff" text with beads-issue-flow's "tracker-sync commits" text, and add a doctor warning when the export is tracked. 04's criterion that "lines 233-234 and 785-788 are made consistent" is the same edit. 04 does not mention 49pg. Add it to Related, remove the consistency edit (or declare 04 blocked by 49pg), and keep only 04's net-new content: the "Snapshot and Dolt remote" section and the three doctor checks.

### MAJOR: 09 closure-orphan-worktree-dirs overlaps open bento-nljv and ignores bento-8oj0's safety bug
bento-nljv already covers closure reporting of orphan dirs under the worktree root, cites the same `shatter/str-6q1i` 109 MB example, and removes only empty dirs. bento-8oj0 shows the doctor's "safe to remove" rule is wrong: it flags slash-branch parent dirs that contain live registered worktrees. 09's removal rule ("not registered worktrees of any repo and no process cwd inside") would delete such a parent and the live worktree in it. Rescope 09 as the non-empty-removal extension of nljv, blocked by nljv and 8oj0. Add a safety criterion that no path containing a registered worktree at any depth is removed, with a fixture test.

### MAJOR: 10 close-reason-evidence and 11 claim-branch-reconciliation overlap the open closure group (bento-wzbt, sy49, 79j2, bo9c, x4bm)
bento-wzbt defines the closure-note contract. bento-x4bm makes land.py validate the note and close the issue itself. 10 requires that "land.py's close step uses the helper", but land.py has no close step until x4bm lands. 11's first criterion ("land.py closes the tracker issue in the same run ... or verify-landing exits non-zero") duplicates x4bm. Neither draft lists these issues. 10 should either become the manual-close enforcement layer of wzbt's contract, blocked by wzbt with its reason forms reconciled with the contract, or fold into x4bm. 11 should drop its first criterion or point it at x4bm, keeping only the stale-claim report.

### MAJOR: 13 per-subagent-scratch-dirs targets templates that do not exist
The audit skill (`catalog/skills/audit/SKILL.md` and its references) has no subagent prompt, and no text mentions subagents or parallel dispatch. `git grep 'proj/'` finds no `proj/` example paths anywhere in `catalog/skills`. Swarm has teammate-prompt requirements (SKILL.md around lines 168-225, CODEX.md line 42), but teammates run in their own worktrees, and the incident came from the shatter audit workflow script, which the draft puts out of scope. As written, 2 of the 3 content criteria (the audit prompt, and replacing `proj/` examples) cannot be met. Name the real target surfaces, or close this as not-a-bento-bug.

### MAJOR: 05 check-unpushed has an acceptance criterion that cannot be checked
"Reports without blocking unless the reflog shows this session created the commits" cannot work: git reflog entries carry no session id, so nothing in the reflog attributes commits to a hook session. Name a real mechanism, such as a per-session record of the HEAD SHA at session start or of commits made (the hook already keeps per-session state keyed on `session_id` from bento-neng), or always report without blocking on the primary branch. Also coordinate with in_progress bento-2p2p, which is editing the same file to decouple the Beads exemptions.

### MINOR: 03 git-hook-latency-visibility test and threshold details
A fixed 30 s threshold makes the "fake slow hook" test take more than 30 s unless the threshold can be configured (env var or argument). Say so. The hook-discovery step should honour `core.hooksPath`, not only `$(git-common-dir)/hooks`. The doctor's marker-age check compares three version strings (0.63.3, 0.56.1 and 1.1.0), so define which comparison triggers the warning.

### MINOR: 04 doctor check (b) adds a `bd` subprocess at SessionStart
`bd dolt remote list` at every SessionStart may be slow; bd writes are known to be slow here. Specify the timeout value and cache the result per session or per day.

### MINOR: 06 doctor-state-per-worktree coordination
In-progress bento-m4y5 changes `.agent-mode.local` key validation and open bento-xy8m fixes the unknown-`hook_bypass` warning; both touch the same code. List them. The draft should also say whether any key must stay per checkout.

### MINOR: 08 repo-scoped stale-preview reporting orphans the leaked previews
The leaked previews point at deleted `/tmp/tmpXXXX/repo` fixture repos, so no repo would ever report them after the change. Add a single "unowned previews" line, or have the closure or removal path handle them.

### MINOR: 12 depends on 10's helper
If 10 is folded into the closure group (above), 12's "close helper refuses to close parents with open children" needs a new home, probably bento-x4bm or the manual flow from bento-wzbt.

### MINOR: bundle-level
The bundle says facts were re-verified at b1bb787, but origin/main is now 5f8b850. Nothing cited changed, but the filer should restate the SHA. The duplicates found above were all filed 2026-09-23, apparently by a parallel audit of the same repo, so rerun a duplicate sweep right before filing.

## Verdict

**Not ready to file as-is.** Drafts 02, 03, 06, 07, 08, 12 and 14 are close to ready after the minor fixes. 01, 04, 09, 10, 11 and 13 need rescoping against open bento issues or corrected targets. 05 needs a real mechanism for its primary-checkout criterion.

Top fixes:
1. Rescope 01 to the residual bypasses, blocked by bento-l01v and ordered after bento-i76i. Move 01's `switch` and `update-ref` additions to i76i or mark them as a deliberate policy change.
2. Reconcile 04, 09, 10 and 11 with bento-49pg, nljv and 8oj0, and the closure group (wzbt, x4bm). List them as Related or Blocked-by and drop the duplicated criteria.
3. Fix the targets of 13 (the audit subagent prompt and `proj/` examples do not exist) and the reflog-based criterion in 05.
