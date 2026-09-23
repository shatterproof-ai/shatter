---
slug: claude-md-imports-agents-md
kind: new
title: "shatter-agents CLAUDE.md points at AGENTS.md without importing it"
priority: P2
type: bug
labels: [agent-guidance, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings (shatter-agents plugin)"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# shatter-agents CLAUDE.md points at AGENTS.md without importing it

## Problem

`CLAUDE.md` at the root of shatter-agents tells the reader that AGENTS.md is the canonical guide, but it does not import it with `@AGENTS.md`. Claude Code loads only CLAUDE.md automatically, so Claude sessions in this repo never see AGENTS.md rules such as "never edit `plugins/` by hand" and "run `scripts/build-plugins`". Unless a session happens to open AGENTS.md itself, it can hand-edit generated payloads, and `check-plugins-clean` then fails in CI.

## Evidence

Re-verified on 2026-09-23 against shatter-agents `119b807`.

- `/home/ketan/project/shatter-agents/CLAUDE.md` in full:
  ```
  # Claude-specific instructions

  See [`AGENTS.md`](AGENTS.md) for the canonical agent guide. This file
  exists so Claude Code's automatic agent-doc discovery finds something
  here; the content of interest is in AGENTS.md.
  ```
  No line is `@AGENTS.md`.
- For comparison, `/home/ketan/project/bugshot/CLAUDE.md` is exactly `@AGENTS.md`.

## Acceptance criteria

- [ ] `CLAUDE.md` imports AGENTS.md: its body is `@AGENTS.md`, optionally followed by Claude-only notes.
- [ ] Proof at close: in a fresh Claude Code session started in the repo, the session can quote the build-plugins / never-edit-`plugins/` rule without reading any file. Record the question and answer in the close comment.

## Suggested approach

Replace the file body with `@AGENTS.md`, as bugshot does.

## Out of scope

A generic check for "CLAUDE.md mentions AGENTS.md without importing it" across repos. That belongs in bento's agent-env doctor (related: bento-m4y5).

## Dependencies

None.

## Priority / Type / Labels

P2 · bug · agent-guidance, audit-2026-09-22

## Source

Shatter audit 2026-09-22, finding plugins-17.
