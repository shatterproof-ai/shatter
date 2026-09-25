---
slug: qwua7-25-refresh-note
kind: note-to-existing
title: "Note on str-qwua7.25: refreshed line-number and .js evidence for the frontend CLAUDE.md files"
priority: P2
type: task
labels: [docs, agents, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.25
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.25 (open, P2: "Slim each frontend CLAUDE.md to ≤5 KB of rules; move per-issue history to docs/frontends/<lang>.md")

Target: `str-qwua7.25`. Post the text below as a comment with `bd comments add str-qwua7.25`. Do not change status or priority.

## Comment text

> **Audit 2026-09-22 (findings frontend-ts-15, frontend-rust-08, docs-24): still open, evidence refreshed at 56c86168.**
>
> This issue's acceptance already owns "line-number citations replaced by symbol names; the two .js references corrected". The re-audit found the same items still present, with drifted locations:
> - `shatter-ts/CLAUDE.md:16-17` cite "Lines ~278-352" (`buildSymExprWithFlow`) and "Lines ~860-951" (`buildSymExpr`). The real definitions are around `instrumentor.ts:874` and `:1862`. Replace with the symbol names.
> - `shatter-ts/CLAUDE.md:379-380` cite `src/browser-globals-recognizer.js` and `src/handlers.js`; the files are `.ts`.
> - `shatter-rust/CLAUDE.md:234-235` cite `src/handler.rs:552, 620` and "line 803". `last_file` is set at `handler.rs:693` and `:767` and read at `:842` and `:970`. Replace with symbol names.
>
> **Proof at close (suggested):** `rg -n ':[0-9]{2,}|Lines? ~?[0-9]' shatter-{ts,go,rust}/CLAUDE.md` returns nothing, pasted into the close note, and the three `.ts`/`.rs` paths resolve.
>
> The non-overlapping stale facts in the crate CLAUDE.md files (core explorer/orchestrator mislabel, Go dead `flow.go`/`ResolveMockSpecs` references, unresolved `str-8v66`/`str-ruw0`, Rust `Runtime::new()` vs `new_current_thread()`, matrix console_output/axum notes) are filed separately as crate-claude-md-stale-facts. Coordinate so the same lines are not edited twice.

## Why a note and not a new issue

The Codex cross-check pointed out that str-qwua7.25's committed acceptance already owns these items; excluding "slimming" from a new issue does not transfer them.

## Source

Audit 2026-09-22, findings frontend-ts-15, frontend-rust-08 and docs-24 (confirmed). Split from crate-claude-md-stale-facts during the cross-check revision.
