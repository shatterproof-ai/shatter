# Plan: Docs Truthfulness Pass (flt-o8l)

## Context
Public-facing docs (README.md, docs/flotsam-user-overview.md) overstate the shipped feature set. File capture is listed as a feature but has no UI. The GraphQL Playground URL is listed without noting it's dev-only.

## Issues Found

### README.md
1. **Line 8-9**: "files" listed alongside bookmarks, voice notes, text notes as a capture type. No file capture UI exists — Capture.tsx has tabs for bookmark, note, voice only. The `FILE` enum exists in GraphQL but is unused in the frontend. **Fix**: Remove "files" from the capture list, or mark it as planned.
2. **Line 44**: GraphQL Playground listed at `/playground` without noting it's only available when `GQLPlayground` config is enabled (dev mode). **Fix**: Add "(dev mode)" qualifier.

### docs/flotsam-user-overview.md
3. **No file capture claim** — this doc correctly doesn't mention file capture. Good.
4. **All other claims verified**: bookmark capture, text note, voice note, Chrome extension, Android app, MCP server, search, tagging — all have corresponding code.

## Changes

### README.md
- Line 8-9: Remove ", and files" from capture description (or change to "and files (planned)")
- Line 44: Change "GraphQL Playground" row to note it's dev-only

### docs/flotsam-user-overview.md
- No changes needed — this doc is already accurate.

## Verification
- Confirm all paths/commands mentioned in README exist (checked: all make targets, directory structure, ports match)
- No code changes, so no test gate needed
