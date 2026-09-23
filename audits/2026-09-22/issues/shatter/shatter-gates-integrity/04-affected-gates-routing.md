---
slug: affected-gates-routing
kind: new
title: "`task affected` misroutes: .md (askama templates, frontend CLAUDE.md) goes only to the hollow `docs` gate, docs-smoke is never selected, core/frontend changes skip cli:test, runtime skips rust-fe:test, shatter-llm falls through to a `check` that doesn't test it"
priority: P2
type: bug
labels: [quality-gates, taskfile, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# `task affected` misroutes several path classes, so the pre-completion gate can pass without running the checks that cover a change

## Problem

The diff-scoped selector `scripts/affected-gates.py` (built under str-35vtk.8) leaves out validators for several classes of path. `task affected` is the pre-completion gate and runs on pre-push, so it can pass without running the checks that cover a change. The selector's tests pin historical merge selections rather than the dependency graph.

## Evidence (re-verified 2026-09-23 by calling `select_gates` directly on the audit snapshot, which is unchanged on main)

| Path | `select_gates([path])` today | Missing |
|---|---|---|
| `shatter-cli/templates/scan.md` | `['docs']` | cli:test, cli:clippy, gauntlet |
| `README.md` | `['docs']` | docs-smoke |
| `shatter-ts/CLAUDE.md` (parity contract) | `['docs']` | parity, conformance |
| `shatter-core/src/report/html.rs` | `['smoke','core:clippy','core:test']` | cli:test |
| `shatter-ts/src/index.ts` | `[..., 'ts:test', 'e2e-ts', 'parity', 'conformance']` | cli:test (build.rs embeds the TS frontend) |
| `shatter-go/main.go` | `[..., 'go:test', 'e2e-go', 'parity', 'conformance']` | cli:test (build.rs embeds the Go frontend) |
| `shatter-rust-runtime/src/lib.rs` | `['smoke','rust-rt:clippy','rust-rt:test','e2e-rust']` | rust-fe:test (executor tests build against the runtime) |
| `shatter-llm/src/jev.rs` | `['smoke','check']` | shatter-llm clippy and tests (`check` runs neither) |

- `scripts/affected-gates.py:126-127` maps any `*.md` or `docs/` path to `{'docs'}` before any crate rule. That makes the templates → gauntlet rule at `:162` unreachable for the `.md` templates.
- `docs-smoke`, which validates README, QUICKSTART, SPEC and docs/INDEX against the built CLI, is in neither `GATE_ORDER` (`:13-38`) nor `_classify` (`:123-184`).
- The `docs` task (`Taskfile.yml:353-378`) consists of `test -f` checks plus markdownlint, vale and lychee. Each of the three prints `[skip] ... not installed` locally and in CI (str-qwua7.46).
- `shatter-llm/` matches no rule, so it falls through to `check` (`:195-200`). `check-static` and `check-unit` run no shatter-llm clippy or tests. CI covers them with separate steps (`.github/workflows/ci.yml:91-103`), and str-35vtk.36 is open to fold them in.

## Acceptance criteria

- [ ] Crate-prefix rules are evaluated before the generic `.md` or `docs/` rule. `shatter-cli/templates/**` selects cli:test, cli:clippy and gauntlet. `shatter-{ts,go,rust}/CLAUDE.md` selects parity and conformance.
- [ ] `README.md`, `QUICKSTART.md`, `SPEC.md`, `docs/INDEX.md` and `scripts/docs-smoke*` select `docs-smoke`, and `docs-smoke` is added to `GATE_ORDER`.
- [ ] `shatter-core/`, `shatter-ts/` and `shatter-go/` paths also select `cli:test`. `shatter-rust-runtime/` also selects `rust-fe:test`.
- [ ] `shatter-llm/` has an explicit rule selecting shatter-llm clippy and test tasks. Create `llm:clippy` and `llm:test` if str-35vtk.36 has not yet. It no longer falls through to `check`.
- [ ] Table-driven cases in `scripts/test_affected_gates.py` cover every row of the table above. They fail on today's tree (record this in the close reason) and pass after the fix. Any new task-name lookup in those tests must not run `task --list-all --json` against the live tree (str-qwua7.3).

## Suggested approach

Reorder `_classify` so crate prefixes win. Add the mappings and the `docs-smoke` gate. Extend the table in `test_affected_gates.py`. Coordinate with str-35vtk.36 (shatter-llm tasks) and str-qwua7.46 (docs linters fail under `CI=1`).

## Out of scope

- Installing or enforcing the doc linters (str-qwua7.46).
- Folding shatter-llm into `task check` (str-35vtk.36).
- Missing Task `sources:` globs (`task-sources-cover-real-inputs`).
- `e2e` running twice in `pre-completion-e2e`, which is filed in `collapse-test-tiers` (shatter-test-hygiene bucket).

## Metadata

- Priority: P2. Type: bug. Size: S-M.
- Labels: quality-gates, taskfile, audit.
- Parent epic: Epic: Audit 2026-09-22 findings.
- Blocked by: none.
- Related: str-qwua7.46, str-35vtk.36, str-35vtk.8.
- Source findings: gates-06, tests-ci-05, frontend-rust-07 (selector part). Drafts shatter-code/04 and shatter-agent/20 (selector half).
