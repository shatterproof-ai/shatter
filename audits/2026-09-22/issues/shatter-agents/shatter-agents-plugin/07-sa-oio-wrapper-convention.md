---
slug: sa-oio-wrapper-convention
kind: note-to-existing
title: "Note on sa-oio: reconcile the wrapper convention with the default-deny execution policy; validate the subcommand in run-shatter"
priority: P2
type: note
labels: [audit-2026-09-22]
parent_epic: ""
blocked_by: []
existing_id: sa-oio
tracker: "bd in /home/ketan/project/shatter-agents (prefix sa)"
---

# Note on sa-oio

Target: **sa-oio** ("Generated wrappers omit CLI subcommand"), OPEN, P1. Post the comment below. It adds evidence and acceptance criteria; it does not change the issue's priority.

## Comment text

Audit 2026-09-22 (finding docs-09) adds evidence and scope to this issue. Re-verified on 2026-09-23 against shatter-agents `119b807`.

- `catalog/skills/add-shatter-target/SKILL.md:80` says "The wrapper command itself should be `shatter` (no extra flags)", and the package.json example at line 92 is `"shatter": "shatter"`. Bare `shatter` prints usage and exits 2 (checked against the current shatter build).
- `catalog/skills/run-shatter/scripts/run_targets.py:119-133` invokes `pnpm/yarn/bun/npm run shatter`, and line 163 invokes `task shatter`, all with no arguments. Nothing checks that a wrapper includes a subcommand.
- `grep -rn "allow-host-writes\|SHATTER_ALLOW_HOST_WRITES" catalog plugins` returns nothing. Shatter has enforced a default-deny host-write policy since str-gg9v (2026-07-09): executing a target is refused without an opt-in (`--allow-host-writes`, `SHATTER_ALLOW_HOST_WRITES`, or a sandbox backend). No skill tells users about that prerequisite.
- A decision is needed. This issue says "Do not inject --allow-host-writes". The audit suggested a canonical wrapper body such as `shatter scan . --allow-host-writes -o shatter-review/report.json`. Pick one, and have add-shatter-target document the execution-safety prerequisite explicitly: either the wrapper carries the opt-in, or the skill tells the user to run under a sandbox or set the variable themselves.
- Proposed additional acceptance:
  - (a) run-shatter fails with a clear message when a wrapper body has no subcommand;
  - (b) a plugin test generates a wrapper for a fixture project and runs it against a real or pinned shatter binary, asserting exit 0 and a report file;
  - (c) every subcommand and flag in the generated wrapper passes cli-contract-test.
- The shatter README's Makefile example (bare `$(SHATTER_BIN)`) is tracked separately in the shatter repo (str-dakf3).
