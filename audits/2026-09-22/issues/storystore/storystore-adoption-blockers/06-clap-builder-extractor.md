---
slug: clap-builder-extractor
kind: new
title: "inventory: extract clap builder-style Command::new(...).subcommand(...) commands as cli-command surfaces"
priority: P3
type: feature
labels: [inventory, coverage, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (storystore)"
blocked_by: [clap-cobra-extractors]
existing_id: ""
tracker: "bd in /home/ketan/project/storystore (prefix ss)"
filer_precondition: "Same as tracker-migration-and-agents-md: stop if `bd list` still reports the v32->v53 migration refusal. File clap-cobra-extractors first so its real id replaces the blocked_by slug."
---

# inventory: extract clap builder-style Command::new(...).subcommand(...) commands as cli-command surfaces

## Problem

clap-cobra-extractors covers clap's derive API only. Rust CLIs that use the
builder API (`Command::new("app").subcommand(Command::new("sync"))`) still
produce no `cli-command` surfaces. Shatter does not use the builder API
(its CLI is derive-only, `shatter-cli/src/args.rs`), so this is for other
consumers and is not an adoption blocker for shatter. Split out of the
pre-revision clap-cobra-extractors draft during the 2026-09-23 cross-check
revision, which found the bundled scope unbounded.

## Evidence

- `shared/inventory.py:140` (storystore HEAD cca768d): the only CLI regex
  is commander.js `.command('name')`.
- Source finding: plugins-05.

## Acceptance criteria

- [ ] Fixtures under `tests/fixtures/` and tests in
      `tests/test_storystore_inventory.py` cover: `Command::new("x")` used as
      the argument of `.subcommand(...)` (emits `x` under its parent, using
      the `parent child` format defined by clap-cobra-extractors), the root
      `Command::new("app")` (not emitted as a command), `.subcommands([...])`
      with several entries, and a `Command::new` not attached to any
      `.subcommand` (not emitted). The new tests fail at the pre-change
      commit and pass after; paste both runs.
- [ ] A file that mixes derive and builder styles yields the union with no
      duplicate names (test).
- [ ] `spec.md` and `shared/spec.md` mention builder support next to the
      derive support documented by clap-cobra-extractors.
- [ ] `python3 -m pytest tests/ -x -q` passes; paste the summary line.

## Out of scope

- Derive API (clap-cobra-extractors). Macro-generated commands.

## Priority

P3: no current consumer is blocked.

## Type / Labels

feature; inventory, coverage, audit-2026-09-22

## Parent epic

Epic: Audit 2026-09-22 findings (storystore)

## Dependencies

- blocked_by: clap-cobra-extractors (reuses its nested-name format, Rust
  file dispatch and per-kind language metadata).
