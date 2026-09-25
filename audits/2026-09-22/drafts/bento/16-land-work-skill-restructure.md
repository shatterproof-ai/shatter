# land-work SKILL.md: restructure around land.py, move manual/batch flow to references, fix $(...) self-contradictions

- Filing action: new issue linked to bento-by8 (by8 trims ~1,500 words across several skills; this is a structural rewrite of one skill). Alternative: append as scope extension to bento-by8.
- Priority: P2
- Type: task
- Labels: audit, land-work, skills
- Parent: epic
- Links: related bento-by8
- Source findings: bento-14

---BODY---
## Problem

`catalog/skills/land-work/SKILL.md` is 6,376 words (44.6 KB), the largest bento skill (swarm is 4,272). Every landing loads the manual 10-step flow, batch mode and the manifest rules, although land.py now drives the serial path. The skill also contradicts itself, and it contradicts consumer repos:

- The Command Rule (about lines 101-106) forbids `$(...)`, but the skill's own commands use it at about lines 143, 181-182 and 449. The use at about line 670 is deliberately escaped and is fine.
- The Tracker Handoff (about lines 783-789) says "issues.jsonl may be intentionally untracked ... do not commit it". Shatter's AGENTS.md requires committing it.

In shatter, bento:land-work was invoked for only about 9 of 35 landings since 09-05. Several other landings were hand-scripted, including 4 raw `git push --no-verify ... :refs/heads/main` by one session.

## Acceptance criteria

- The serial path in SKILL.md reads: run land.py (see the invocation block from the land.py-invocation issue), read the JSON, fix the named step. The manual flow, batch mode and manifest rules move to `references/`.
- SKILL.md word count is at most 2,500 (`wc -w`).
- No `$(...)` in any command the skill tells the agent to run. The computations move into scripts.
- The Tracker Handoff defers to the repo's documented jsonl policy instead of prescribing one.
- A skill lint or test asserts that SKILL.md has no unescaped `$(` in fenced command blocks.

## Out of scope

- Behaviour changes to land.py.
