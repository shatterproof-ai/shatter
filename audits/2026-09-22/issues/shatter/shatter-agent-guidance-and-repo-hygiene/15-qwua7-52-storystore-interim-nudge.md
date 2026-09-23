---
slug: qwua7-52-storystore-interim-nudge
kind: note-to-existing
title: "Note on str-qwua7.52: choose adopt-now vs wait for storystore fixes, and set a dated remind_after meanwhile"
priority: P2
type: chore
labels: [agents, stories, tooling]
parent_epic: "(existing issue; parent str-qwua7)"
blocked_by: []
existing_id: str-qwua7.52
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.52: interim storystore nudge

Target: **str-qwua7.52** (open, P2, "Adopt storystore: initialise
docs/stories, seed CLI stories, land str-u394l.3"). Action:
`bd comments add str-qwua7.52` with the text below. Do not change its
priority. Sibling of the str-qwua7.53 note (`env-doctor-decisions`); split
from the first-revision draft of that slug.

## Comment text

> Audit 2026-09-22 (findings agent-repo-15, plugins-08;
> `audits/2026-09-22/findings.json`, `audits/2026-09-22/areas/plugins-guidance.md`).
>
> - Still dormant: `/home/ketan/project/shatter/docs/stories` does not exist,
>   and every SessionStart prints "storystore dormant — decision pending".
> - storystore's inventory finds **0 clap surfaces** in shatter (storystore
>   extractor gap) and the installed storystore plugin cache is stale. Both are
>   storystore tracker items. The extractor gap limits only automatic
>   CLI-surface completeness reporting; it does not block `stories-init`,
>   seeding or str-u394l.3's acceptance (details and a proposed seed list are
>   in the sibling audit comment from `qwua7-52-story-seed-list`).
> - The bento doctor supports a dated reminder:
>   `agent_env_doctor_remind_after=<plugin>:<YYYY-MM-DD>[,...]`
>   (`/home/ketan/project/bento/plugins/claude/bento/hooks/scripts/agent-env-doctor.py:69,72`
>   recognised keys; value format at `:997-999`).
>
> **Proposed interim checkpoint (this issue stays open for adoption):**
> 1. Record in this issue which way adoption goes first: (a) run
>    `storystore:stories-init` now and accept observed-mode stories without
>    CLI surfaces, or (b) wait for the storystore extractor and cache fixes
>    (link those storystore issues here).
> 2. With (b), add `agent_env_doctor_remind_after=storystore:<date>` to
>    `/home/ketan/project/shatter/.agent-mode.local` (untracked; record the
>    action) with a date no more than 60 days out, and paste the doctor's
>    SessionStart output before (storystore pending) and after (not shown).
> 3. Adoption (this issue's own acceptance) is unchanged. Note for the
>    ordering: <completion-checklist-spec-docs> makes
>    `storystore:stories-impact-check` a pre-edit step once
>    `docs/stories/INDEX.md` exists, so adoption also switches that on.
