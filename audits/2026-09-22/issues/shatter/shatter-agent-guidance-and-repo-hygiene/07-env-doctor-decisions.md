---
slug: env-doctor-decisions
kind: note-to-existing
title: "Note on str-qwua7.53: its interim step (agent_env_doctor_skip_plugin=bugshot) was never applied; do it now, independent of bgs-3tq"
priority: P2
type: chore
labels: [agents, tooling]
parent_epic: "(existing issue; parent str-qwua7)"
blocked_by: []
existing_id: str-qwua7.53
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.53: apply the bugshot interim step now

Target: **str-qwua7.53** (open, P2, "Wire bugshot for walkthrough output once
bugshot supports CLI capture (bgs-3tq)"). Action: `bd comments add str-qwua7.53`
with the text below. Do not change its priority.

This draft was a new issue in the first revision. The cross-check found that
str-qwua7.53 and str-qwua7.52 already own these decisions and their
"no SessionStart nudge" acceptance, so it is now a note here plus a sibling
note on str-qwua7.52 (`qwua7-52-storystore-interim-nudge`). The orphan
worktree directories that the old draft also covered moved to
`orphan-worktree-dirs-cleanup`.

## Comment text

> Audit 2026-09-22 (findings agent-repo-15, plugins-08;
> `audits/2026-09-22/findings.json`).
>
> This issue's decision says "Until then set
> `agent_env_doctor_skip_plugin=bugshot` in `.agent-mode.local` so the nudge
> stops". That interim step was never applied, so every SessionStart still
> prints "bugshot dormant — decision pending", and agents learn to skip
> SessionStart output (hiding new warnings).
>
> Evidence, re-verified 2026-09-23:
> - `cat /home/ketan/project/shatter/.agent-mode.local` -> `dangerous`,
>   `agent_env_doctor_seen=bugshot,storystore`,
>   `agent_env_doctor_superpowers_pointer_seen=true`. No
>   `agent_env_doctor_skip_plugin` key.
> - The doctor recognises the key:
>   `/home/ketan/project/bento/plugins/claude/bento/hooks/scripts/agent-env-doctor.py:69,72`
>   (`RECOGNIZED_AGENT_MODE_KEYS`).
> - Cross-repo dependency: the permanent wiring waits on bugshot **bgs-3tq**
>   (CLI capture template). Its priority raise is proposed separately in
>   the bugshot tracker.
>
> **Proposed checkpoint inside this issue (do it now; it does not wait on
> bgs-3tq, and this issue stays open for the real wiring):**
> 1. Add `agent_env_doctor_skip_plugin=bugshot` to
>    `/home/ketan/project/shatter/.agent-mode.local` (untracked per-checkout
>    state; record the action, there is no commit).
> 2. Proof: run the doctor with the same invocation its SessionStart hook
>    uses (bento `hooks.json`) from the primary checkout, before and after,
>    and paste both outputs: bugshot appears before and not after.
> 3. Known limit: linked worktrees get their own `.agent-mode.local`, so
>    they still show the nudge. That is a bento-side bug tracked in the bento
>    audit bucket; note it, do not work around it here.
