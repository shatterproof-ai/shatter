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
