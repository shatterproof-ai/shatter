# `task affected` misroutes: .md (incl. askama templates and frontend CLAUDE.md) goes only to the hollow `docs` gate, docs-smoke is never selected, core/frontend changes skip cli:test, shatter-llm falls through to a `check` that doesn't test it

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P2 |
| labels | quality-gates,taskfile,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-qwua7.46, str-35vtk.36, str-35vtk.8 |
| source findings | gates-06, tests-ci-05, frontend-rust-07 (selector part) |

<!-- body -->
## Problem

The diff-scoped selector `scripts/affected-gates.py` misses validators for several path classes, so `task affected` (the pre-completion gate) can pass without running the checks that cover a change.

## Current code facts / evidence

- `scripts/affected-gates.py:125-127` maps any `*.md` or `docs/` path to `{'docs'}` before crate rules, so `shatter-cli/templates/scan.md` → `['docs']` and the templates→gauntlet rule at :162 is unreachable; frontend `CLAUDE.md` parity-contract edits skip parity/conformance.
- `docs-smoke` (validates README/QUICKSTART/SPEC/docs/INDEX against the built CLI) is in neither `GATE_ORDER` (:13-37) nor `_classify`.
- The `docs` task (`Taskfile.yml:353-378`) is `test -f` checks plus markdownlint/vale/lychee, which print `[skip] ... not installed` locally and in CI.
- `select_gates(['shatter-core/src/report/html.rs'])` → `['smoke','core:clippy','core:test']` (no cli:test); `shatter-ts/src/index.ts` and `shatter-go/main.go` also select no cli:test.
- `select_gates(['shatter-rust-runtime/src/lib.rs'])` → no rust-fe:test; `select_gates(['shatter-llm/src/jev.rs'])` → `['smoke','check']`, and check-static/check-unit run no shatter-llm clippy/tests.

## Acceptance criteria

- Crate rules are evaluated before the generic `.md` rule; askama templates select cli:test + gauntlet; frontend CLAUDE.md selects parity + conformance.
- README.md, QUICKSTART.md, SPEC.md, docs/INDEX.md and scripts/docs-smoke* select `docs-smoke`.
- shatter-core and frontend paths select cli:test; shatter-rust-runtime selects rust-fe:test; shatter-llm selects its own clippy+test tasks.
- Table-driven tests in `scripts/test_affected_gates.py` cover every path above.

## Suggested approach

Reorder `_classify`, add the mappings, add tests. Coordinate with str-35vtk.36 (fold shatter-llm into check) and str-qwua7.46 (docs linters fail under CI=1).

## Scope

- In scope: the acceptance criteria above.
- Out of scope: Installing doc linters (str-qwua7.46); folding shatter-llm into `task check` (str-35vtk.36).
- Size: S-M

## References

- Audit findings: gates-06, tests-ci-05, frontend-rust-07 (selector part) (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-qwua7.46, str-35vtk.36, str-35vtk.8
