# Unblock the storystore tracker (pending v32->v53 bd schema migration) and add AGENTS.md/CLAUDE.md

## Filing metadata

- tracker/repo: storystore
- action: create new issue
- type: chore
- priority: P2
- labels: audit-2026-09-22
- parent: repo epic (see INDEX)
- dedupe relation: new
- source findings: plugins-09

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
## Problem

`bd list` in `/home/ketan/project/storystore` prints "refusing to auto-apply 21 pending schema migrations to a remote-backed database (v32 -> v53) … Writes are blocked until the schema is reconciled", and also "beads.role not configured". No issue can be filed or updated. The repo root has README.md, spec.md, INSTALL.md and four top-level `2026-05-01-storystore-plan-*.md` / `-target-design.md` files, but no AGENTS.md or CLAUDE.md, so agents get no conventions.

## Acceptance criteria

- [ ] With user approval, one designated clone runs the migration (`BD_ALLOW_REMOTE_MIGRATE=1 bd migrate && bd dolt push`), other clones re-bootstrap, and `beads.role` is set. `bd create` works.
- [ ] `AGENTS.md` documents build-plugin, the version-bump rule, tests and landing. `CLAUDE.md` is `@AGENTS.md`.
- [ ] The plan docs move to `docs/plans/`.

## Note

The migration is a coordination decision that forks schema if done per clone. It needs explicit maintainer approval before running. Because writes are blocked, this issue may have to be filed after the migration, or recorded elsewhere until then.

## Source

Shatter audit 2026-09-22 finding plugins-09.
