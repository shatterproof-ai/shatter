---
slug: tracker-migration-verification
kind: new
title: "Record the v32->v53 bd schema migration, bootstrap every other storystore clone, and set beads.role"
priority: P2
type: chore
labels: [beads, tracker, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (storystore)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/storystore (prefix ss)"
filer_precondition: "Same as tracker-migration-and-agents-md: stop if `bd list` still reports the v32->v53 migration refusal. The migration itself is done by the maintainer before filing; this issue records it and finishes the per-clone follow-up."
---

# Record the v32->v53 bd schema migration, bootstrap every other storystore clone, and set beads.role

## Problem

On 2026-09-23, `bd list` in `/home/ketan/project/storystore` (bd 1.1.0)
printed:

```
Warning: refusing to auto-apply 21 pending schema migrations to a remote-backed database (v32 -> v53): migrating clones independently forks the schema (#4259)
  Read-only command: continuing on schema v32 without migrating.
  Writes are blocked until the schema is reconciled.
  ...
    • designated migrator (only ONE machine): BD_ALLOW_REMOTE_MIGRATE=1 bd migrate && bd dolt push
    • every other clone (another already migrated): bd bootstrap
warning: beads.role not configured (GH#2950).
  Fix: git config beads.role maintainer
  Or:  git config beads.role contributor
```

The database is remote-backed (`bd dolt remote list` shows
`origin git+ssh://git@github.com/ketang/storystore.git`), so migrating more
than one clone would fork the schema. The maintainer runs the migration on
one clone before this bucket is filed (see `filer_precondition`). What is
left after filing: a durable record of which clone migrated, `bd bootstrap`
on every other clone, and `beads.role` set in each clone.

## Evidence (re-verified 2026-09-23 against storystore HEAD cca768d)

- `bd list` / `bd show ss-yoa`: the refusal and `beads.role` warning quoted
  above, on every read.
- Source finding: plugins-09. Split out of tracker-migration-and-agents-md
  during the 2026-09-23 cross-check revision.

## Acceptance criteria

- [ ] The close reason names the designated migrator clone (host + path)
      and when `BD_ALLOW_REMOTE_MIGRATE=1 bd migrate && bd dolt push` ran,
      with `bd list 2>&1 | head -3` output from that clone that has no
      "refusing to auto-apply" line.
- [ ] Every other storystore clone the maintainer uses ran `bd bootstrap`
      (not `bd migrate`). The close reason lists each clone with its own
      `bd list 2>&1 | head -3` output free of the refusal, or says "no other
      clones".
- [ ] In each listed clone, `git config beads.role` prints `maintainer` or
      `contributor`, and `bd list` no longer prints "beads.role not
      configured". Paste the output.
- [ ] A write round-trip succeeds on a non-migrator clone after
      `bd dolt pull`: create a throwaway issue, `bd dolt push`, see it from
      the migrator clone after `bd dolt pull`, then delete it. Paste the ids
      and commands.

## Out of scope

- AGENTS.md / CLAUDE.md and the plan-doc move
  (tracker-migration-and-agents-md).
- Any `BEADS_HOOK_TIMEOUT` or hook-bypass guidance (excluded by D4).

## Priority

P2: a second, unbootstrapped clone can fork the schema or keep writes
blocked for whoever uses it.

## Type / Labels

chore; beads, tracker, audit-2026-09-22

## Parent epic

Epic: Audit 2026-09-22 findings (storystore)

## Dependencies

- blocked_by: none.
