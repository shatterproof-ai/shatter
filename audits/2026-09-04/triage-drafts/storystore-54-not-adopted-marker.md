---
repo: storystore
tracker: beads-blocked
type: feature
priority: 3
existing: none
---
# Provide an explicit "not adopted" marker recognised by doctors and patrols

## Problem
When storystore is installed globally but a repo has not decided to use it, two independent systems nag forever: bento's `agent-env-doctor` prints "storystore is installed but dormant — docs/stories is missing" every session, and a consumer's drift patrol (shatter's `scripts/drift-patrol.py check_docs_stories`) reports a permanent `PENDING` because `docs/stories` does not exist. In the shatter repo this has run at every session for two months while the adoption decision (tracked as shatter issue str-u394l.3) sits open. There is no way to record "considered, not adopted (yet)" that both sides understand; the only silence is bento's `agent_env_doctor_skip_plugin=storystore`, which storystore's own docs never mention.

## Current code facts
- `~/project/storystore/skills/stories-init/SKILL.md` L8–63: Phase 1 creates `docs/stories/`, `README.md`, `INDEX.md`, gitignores `drift-todo.md`; it is idempotent but has no "decline" path and writes nothing for a repo that opts out.
- `~/project/storystore/README.md`: adoption is described only as running `stories-init`; no mention of `.agent-mode.local` or of how downstream doctors detect the plugin.
- bento's detection is data-driven: `agent-env-doctor.py` `PLUGIN_PRECONDITIONS` checks `docs/stories` as a directory; a marker file would need a matching precondition change on the bento side (coordinate).
- storystore's beads DB refuses commands (21 pending schema migrations on a remote-backed DB); this draft is filed as `beads-blocked` until the tracker is reconciled.

## Decision for maintainer
- Marker location. Proposal: an `.agent-mode.local` key, `storystore=not-adopted[;reason=...;revisit_after=YYYY-MM-DD]`, **not** a file under `docs/stories/`. Reason: bento's doctor precondition is "`docs/stories` is a directory", so any file under it would make the plugin look wired (silencing the nudge by accident and making `stories-audit`/`stories-coverage` treat the repo as active); `.agent-mode.local` is already the file bento reads for opt-outs (`agent_env_doctor_skip_plugin`), so one key serves both sides. Alternative: `docs/.stories-not-adopted` if a committed, reviewable marker is preferred.
- `revisit_after` semantics: proposal — `stories-audit` reports "declined, revisit date passed" once the date passes (report only, never re-nag from SessionStart).
- Filing: storystore's beads DB is blocked; until reconciled this draft is held in the shatter audit bundle and should be filed when `bd` works again (or as a GitHub issue if the repo has one).

## Acceptance checks
- `stories-init --decline "<reason>" [--revisit-after YYYY-MM-DD]` writes the `.agent-mode.local` key and nothing else; `stories-init` without the flag removes the key when it creates the real scaffold.
- README documents the key and states that `agent_env_doctor_skip_plugin=storystore` remains the bento-side hard silence for repos that will never adopt.
- `stories-audit`/`stories-coverage` treat a repo with only the marker as "declined" (exit 0, one-line report), not as an error.
- A short doc note lists what downstream checks should do with the marker (bento doctor: silent; drift patrols: `SKIP`, not `PENDING`), and a bento follow-up issue is referenced once filed.

## Scope
In: stories-init flag, marker format, README, audit/coverage handling, tests.
Out: changing bento's doctor (separate bento issue); revisiting story authority rules.

## Size
small

## Provenance
Shatter audit 2026-09-04 (shatter repo, branch audit-2026-09-04, audits/2026-09-04.md item 54); evidence summary inline above.
