---
slug: env-doctor-decisions
kind: new
title: "Apply the 2026-09-06 storystore/bugshot decisions to .agent-mode.local and remove the doctor-flagged orphan worktree dirs"
priority: P2
type: chore
labels: [agents, stories, tooling, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Apply the 2026-09-06 storystore/bugshot decisions to .agent-mode.local and remove the doctor-flagged orphan worktree dirs

## Problem

The bento agent-env-doctor (a SessionStart hook) prints the same warnings every
session, and every session ignores them:

- "storystore dormant — decision pending"
- "bugshot dormant — decision pending"
- five orphan worktree directories

The maintainer decided both plugin questions on 2026-09-06:

- adopt storystore (str-qwua7.52);
- silence bugshot until bugshot's CLI capture (bgs-3tq) lands. str-qwua7.53
  says: "Until then set agent_env_doctor_skip_plugin=bugshot".

Neither decision was ever written into `.agent-mode.local`, the file that
encodes them. Warnings that repeat unchanged teach agents to skip
SessionStart output, which also hides new, real warnings.

## Evidence

Re-verified 2026-09-23:

- `cat /home/ketan/project/shatter/.agent-mode.local` ->
  `dangerous`, `agent_env_doctor_seen=bugshot,storystore`,
  `agent_env_doctor_superpowers_pointer_seen=true`. There is no
  `agent_env_doctor_skip_plugin` or `agent_env_doctor_remind_after` key.
- The doctor recognises both keys:
  `/home/ketan/project/bento/plugins/claude/bento/hooks/scripts/agent-env-doctor.py:69,72`
  (`RECOGNIZED_AGENT_MODE_KEYS`). `remind_after` takes
  `<plugin>:<YYYY-MM-DD>[,...]` (`:997-999`), and the pending text is built at `:537`.
- Orphan dirs, none of them a git repo any more:
  `~/.local/share/worktrees/shatter/str-6q1i` (109 MB),
  `str-hszo-tmpfix` (573 MB), `str-k6e61-scm-followups` (16 KB),
  `str-mambd-enum-variant-gen` (16 KB), `str-yhsp-concolic-run` (16 KB).
- `/home/ketan/project/shatter/docs/stories` does not exist. str-qwua7.52 and
  str-qwua7.53 are open and unclaimed (created 2026-09-07).
- Storystore today cannot see shatter's CLI: its inventory finds 0 clap
  surfaces (storystore extractor gap), and the installed plugin cache is stale
  (audit area `plugins-guidance.md`).
- Linked worktrees get their own `.agent-mode.local`, so they show the full
  nudges even after the primary is fixed. That is a bento bug, tracked in the
  bento audit bucket.
- Audit sources: agent-repo-15, plugins-08 (`audits/2026-09-22/findings.json`).

## Acceptance criteria

1. `agent_env_doctor_skip_plugin=bugshot` is present in
   `/home/ketan/project/shatter/.agent-mode.local`. Proof: run the doctor
   with a SessionStart payload from the primary checkout; its output no longer
   mentions bugshot (paste the output in the close reason).
2. Storystore: pick one and record it in this issue:
   (a) run `storystore:stories-init` under str-qwua7.52 once the storystore
   clap extractor and stale-cache problems are fixed; or
   (b) set `agent_env_doctor_remind_after=storystore:<YYYY-MM-DD>` with the
   reason "blocked on storystore clap extractor and stale plugin cache",
   linking the storystore issues in the comment.
   With (b), the doctor output no longer shows storystore as pending before
   that date (paste the output).
3. The five orphan dirs are removed **only after explicit operator
   confirmation**, with sizes recorded in the issue. Afterwards
   `ls ~/.local/share/worktrees/shatter/` no longer lists them, and the doctor
   stops reporting them.
4. A comment on str-qwua7.53 records the cross-repo dependency on bgs-3tq.
   (The bgs-3tq priority raise is filed in the bugshot bucket, not here.)

## Suggested approach

`.agent-mode.local` is untracked, per-checkout state, so the edit is an
operator/agent action recorded in the close reason, not a commit. Test the
doctor with the same invocation its SessionStart hook uses (see bento
`hooks.json`).

## Out of scope

- The bento doctor's per-worktree state location and escalation behaviour
  (bento tracker).
- Storystore extractor or plugin-cache fixes (storystore tracker).
- Raising bgs-3tq's priority (bugshot bucket).
- The `.claude/worktrees/str-umw3` orphan (`agent-config-gitignore`).

## Dependencies

None. Cross-repo relations (bd cannot express them): bgs-3tq (bugshot) and the
storystore clap-extractor issue.
