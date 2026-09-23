# Decide and apply beads hook timeout policy (post-checkout ~300s per worktree/preview; str-qwua7.28 vs str-mpgg1 deadlock)

- Priority: P1
- Type: decision
- Labels: agents,beads,git-hooks,landing
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new decision issue (partially covered by str-qwua7.28; conflicts with str-mpgg1; append note to str-qwua7.28)
- Source findings: sessions-05, agent-repo-07
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
The beads `post-checkout` hook times out at its 300 s default on landing
preview worktrees and new worktrees, adding ~4-5 minutes to every landing and
launch. The fix proposed in str-qwua7.28 (lower `BEADS_HOOK_TIMEOUT` via
`scripts/setup-hooks.sh`) contradicts str-mpgg1, which reverted exactly that
hook-env edit as "unauthorized". No one has decided between them, so agents
keep improvising bypasses.

## Current Code Facts
- `.git/hooks/post-checkout:6` and `pre-push:6`:
  `_bd_timeout=${BEADS_HOOK_TIMEOUT:-300}`; marker "BEADS INTEGRATION v0.63.3";
  installed bd is 1.1.0.
- `grep -rn BEADS_HOOK_TIMEOUT scripts/ .beads/hooks Taskfile.yml` -> nothing.
  str-qwua7.28's body claims `scripts/setup-hooks.sh:41` defines it — stale
  since b5cd25ec (str-mpgg1 revert, merged 84941b37).
- land.py `create_preview` durations: 234.9-301.2 s on 11 landings (09-19..22);
  transcripts contain "hook 'post-checkout' timed out after 300s" 32 times.
- `time bd hooks run post-checkout` in a linked worktree: 2m59.7s.
- Hydration warning seen: "post-checkout JSONL import warning: no Dolt remote
  configured".
- str-mpgg1 is still `in_progress` though merged (see draft 10).

## Decision Needed (maintainer)
Pick one and record it in AGENTS.md:
(a) export `BEADS_HOOK_TIMEOUT=30` via session env (`.envrc` or `env` in
    `.claude/settings.json`) — edits no hook file;
(b) allow a shatter-managed env block in the hooks (reverses str-mpgg1);
(c) root-cause and fix bd hook latency (upstream beads) and skip hydration in
    `/tmp/land-work-preview-*`.

## Acceptance Criteria
- Decision recorded here and in AGENTS.md "Leave the managed git hooks alone".
- After applying it, `git worktree add` in the shatter repo completes in
  < 15 s (measured; command and time in close reason) and land.py
  `create_preview` < 30 s.
- str-qwua7.28 body corrected (the setup-hooks.sh:41 fact) and either closed
  as superseded or re-scoped; str-mpgg1 closed.
- Bypass guidance removed from memory (draft 07).

## Out of Scope
Pre-commit hook cost (separate L4 finding). bento-side hydration suppression
(bento tracker).
