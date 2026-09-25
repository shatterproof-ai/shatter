---
slug: go-cobra-extractor
kind: new
title: "inventory: extract Go cobra commands (&cobra.Command{Use: ...} plus AddCommand nesting) as cli-command surfaces"
priority: P3
type: feature
labels: [inventory, coverage, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (storystore)"
blocked_by: [clap-cobra-extractors]
existing_id: ""
tracker: "bd in /home/ketan/project/storystore (prefix ss)"
filer_precondition: "Same as tracker-migration-and-agents-md: stop if `bd list` still reports the v32->v53 migration refusal. File clap-cobra-extractors first so its real id replaces the blocked_by slug."
---

# inventory: extract Go cobra commands (&cobra.Command{Use: ...} plus AddCommand nesting) as cli-command surfaces

## Problem

storystore detects Go (`go.mod`) but has no Go CLI extractor, so Go CLIs
built with cobra produce no `cli-command` surfaces. Shatter has no cobra
dependency (no `spf13/cobra` in any `go.mod` in the shatter tree), so this
is for other consumers and is tested with fixtures only. Split out of the
pre-revision clap-cobra-extractors draft during the 2026-09-23 cross-check
revision.

## Evidence

- `shared/inventory.py:61` (storystore HEAD cca768d):
  `EXTRACTED_LANGUAGES = frozenset({"typescript", "javascript"})`;
  `build_inventory` (`:336`) has no `.go` branch.
- Source finding: plugins-05.

## Acceptance criteria

- [ ] Fixtures under `tests/fixtures/` and tests in
      `tests/test_storystore_inventory.py` cover: `&cobra.Command{Use: "name [args]"}`
      (name = first word of `Use`); a root command (the one passed to
      `Execute()` or never `AddCommand`-ed) not emitted as a command;
      `parent.AddCommand(child)` nesting emitted as `parent child` (format
      from clap-cobra-extractors) when both are in the same package
      directory; a `Use` held in a const or built dynamically is skipped and
      reported, not guessed. The new tests fail at the pre-change commit and
      pass after; paste both runs.
- [ ] Go files under `vendor/` and `_test.go` files are skipped (test).
- [ ] The per-kind language metadata reports `go` as extracted for
      `cli-command` only when this extractor exists (test).
- [ ] `spec.md` and `shared/spec.md` document cobra support and its limits.
- [ ] `python3 -m pytest tests/ -x -q` passes; paste the summary line.

## Out of scope

- stdlib `flag`, urfave/cli, kong. Cross-package `AddCommand` resolution.

## Priority

P3: no current consumer is blocked.

## Type / Labels

feature; inventory, coverage, audit-2026-09-22

## Parent epic

Epic: Audit 2026-09-22 findings (storystore)

## Dependencies

- blocked_by: clap-cobra-extractors (nested-name format and per-kind
  language metadata).
