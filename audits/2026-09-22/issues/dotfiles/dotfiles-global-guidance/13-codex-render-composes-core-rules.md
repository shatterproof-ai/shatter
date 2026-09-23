---
slug: codex-render-composes-core-rules
kind: new
title: "Deliver the core-rules file to Codex: make agents-sync.sh render, status and adopt composition-aware"
priority: P1
type: bug
labels: [bug]
parent_epic: "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"
blocked_by: [global-guidance-actually-loads]
existing_id: ""
tracker: "gh -R ketang/dotfiles (GitHub Issues; no .beads in the repo)"
---

# Deliver the core-rules file to Codex: make agents-sync.sh render, status and adopt composition-aware

Part of #<epic>. Priority: P1. Type: bug. Split from `global-guidance-actually-loads`, which creates `docs/agent-guidance/core.md` and injects it into Claude Code sessions. This issue is blocked by it.

## Problem

Codex sessions read only `~/.codex/AGENTS.md`, which `codex/agents-sync.sh render` copies from `codex/AGENTS.md`. The Required-Loads leaves are prose pointers there too, so Codex sessions get the same ~0% load rate as Claude sessions (see `global-guidance-actually-loads`). Codex has no SessionStart hook, so the core rules have to be in the rendered file.

The render/adopt contract cannot just append a file. At dotfiles @ `81f35e1`:

- `cmd_render` (`codex/agents-sync.sh:42-63`) copies `BASE` to both `TARGET` and `SNAPSHOT`.
- `cmd_status` (`:65-89`) reports `render vs snapshot: pending` whenever `SNAPSHOT` differs from `BASE`. If render appended `core.md`, status would report pending forever.
- `cmd_adopt` (`:91-114`) copies the whole live `TARGET` back into `BASE`. After a composed render it would copy `core.md`'s content into `codex/AGENTS.md`, and the next render would append it a second time.

Also, `render vs snapshot: ok` compares only `BASE` and `SNAPSHOT`. It says nothing about the referenced leaves, and it can be `ok` while `live vs snapshot` is `drifted`. It is not proof that a rule reached Codex.

## Acceptance criteria

- [ ] `render` writes `BASE` followed by a delimited block (begin and end marker lines) containing `core.md` to `TARGET`, and writes the same composed output to `SNAPSHOT`.
- [ ] `status` compares `SNAPSHOT` with the composed output that render would produce now (`BASE` plus the current `core.md`). A change to `core.md` alone reports `pending`.
- [ ] `adopt` strips the delimited core block from the live file before writing `BASE`. Live edits inside the block are reported and not written into `BASE` (the fix belongs in `core.md`).
- [ ] Alternatively, if the maintainer prefers that `core.md`'s rules be written directly into `codex/AGENTS.md` within dotfiles#23's 800-word budget, the closing comment records that choice, and the content criterion below still applies.
- [ ] Round-trip tests in a new `test/agents-sync-compose-test.sh`, run with `bash test/agents-sync-compose-test.sh` (the `test/*-test.sh` scripts are standalone; there is no shared runner). Each case sets `HOME` to a temp directory:
  - render, then adopt, leaves `BASE` byte-identical;
  - render, adopt and render again gives exactly one core block in `TARGET`;
  - editing `core.md` makes `status` exit 1 with `pending`;
  - a live edit outside the block is adopted into `BASE`, and a live edit inside it is not.

  The tests must fail on the current script.
- [ ] Content check: after `render`, `~/.codex/AGENTS.md` contains the core marker line and every `##` heading of `core.md`.

## Proof at close

The closing comment includes the red and green test runs, `grep -c '<core marker>' ~/.codex/AGENTS.md` returning 1, and the marker found in a Codex session started after the render: `grep -l '<core marker>' ~/.codex/sessions/<yyyy>/<mm>/<dd>/rollout-*.jsonl` (rollout files record the loaded instructions).

## Out of scope

- Automating or surfacing the render after guidance changes (dotfiles#21).
- The contents of `core.md` (`global-guidance-actually-loads`).

## Dependencies

Blocked by `global-guidance-actually-loads` (creates `core.md`). Coordinate with dotfiles#21 and dotfiles#23, which touch the same render path and file.

## Source

Shatter audit 2026-09-22, finding plugins-06. Split out during the Codex cross-check of 2026-09-23.
