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
