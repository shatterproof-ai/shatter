# Apply the 2026-09-06 storystore/bugshot decisions to .agent-mode.local and clean doctor-flagged orphan worktrees

- Priority: P2
- Type: chore
- Labels: agents,stories,tooling
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (partially covered by str-qwua7.52, str-qwua7.53, str-qwua7.1)
- Source findings: agent-repo-15, plugins-08
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
The bento agent-env-doctor prints the same warnings every session and they are
ignored: "storystore dormant — decision pending", "bugshot dormant — decision
pending", and five orphan worktree directories. The maintainer decided on
2026-09-06 to adopt storystore (str-qwua7.52) and to silence bugshot until
bugshot's CLI capture lands (str-qwua7.53: "Until then set
agent_env_doctor_skip_plugin=bugshot"), but neither was applied.

## Current Code Facts
- Primary `.agent-mode.local` contains only `dangerous`,
  `agent_env_doctor_seen=bugshot,storystore`,
  `agent_env_doctor_superpowers_pointer_seen=true` — no skip key.
- Linked worktrees get their own `.agent-mode.local` (per-checkout state; a
  bento bug), so they show full nudges.
- Orphan dirs (Jun-Jul 2026): `~/.local/share/worktrees/shatter/{str-6q1i,
  str-hszo-tmpfix,str-k6e61-scm-followups,str-mambd-enum-variant-gen,
  str-yhsp-concolic-run}` (~682 MB total).
- bugshot prerequisite `bgs-3tq` is P3 while shatter str-qwua7.53 is P2.
- `docs/stories/` absent; storystore's inventory currently finds 0 clap CLI
  surfaces in shatter (storystore extractor gap) and its installed cache is 128
  commits stale.

## Acceptance Criteria
- `agent_env_doctor_skip_plugin=bugshot` set in the primary `.agent-mode.local`
  (and doctor output confirms bugshot is silent).
- storystore: either run `storystore:stories-init` under str-qwua7.52, or set a
  `remind_after` date with the reason "blocked on storystore clap extractor and
  stale plugin" and link the storystore issues.
- The five orphan dirs removed after operator confirmation (sizes recorded).
- A note on str-qwua7.53 and bgs-3tq records the cross-repo priority mismatch.

## Out of Scope
bento doctor state location fix; storystore extractor work (their trackers).
