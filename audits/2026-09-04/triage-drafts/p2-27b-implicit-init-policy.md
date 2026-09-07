---
repo: shatter
type: task
priority: 2
labels: cli, explore, config
existing: none
---
# Keep implicit init; document it and add --no-init / SHATTER_NO_INIT=1
## Decision (2026-09-06)
Option 1: keep implicit init and make it explicit in docs. SPEC §2.1/§2.8 state that execution commands create `.shatter/config.yaml` (and the gitignore block unless tracked) when absent; add `--no-init` (and honour `SHATTER_NO_INIT=1`) for read-only use, with a test that a fresh directory is untouched when opted out. Setup lines to stderr is tracked separately (str-qwua7.39).

## Problem
A first `shatter explore file.ts:fn` in a directory with no `.shatter/` silently writes `.shatter/config.yaml` and a managed `.gitignore` block into the cwd. README says installing and initializing "are separate steps" and SPEC §2.8 presents `init` as the explicit opt-in; SPEC §2.1 does not mention that explore initializes. Users running Shatter against a checkout they do not own get unexpected files.

## Current code facts
- `shatter-cli/src/main.rs:43-52` `run_implicit_init` runs before dispatch for the initialized-project path; `init.rs:16-38` doc: implicit init "still creates `.shatter/` and a *new* `.gitignore` exactly like an explicit `init` (that preserves today's first-run ergonomics), but if `.gitignore` is already tracked it is left alone" (str-w5jt9, `tests/implicit_init_gitignore_test.rs`).
- README.md "Initialize a Project": "Installing … and initializing a project are separate steps"; `docs/PROJECT-LAYOUT.md:44-47` "some commands will initialize it implicitly".
- Other commands also write `.shatter-cache/` and `shatter-artifacts/` on demand (SPEC §2.8 "Effect on the project tree").

## Options
1. **Keep implicit init, make it explicit in docs (proposed default)**: SPEC §2.1 and §2.8 state that execution commands create `.shatter/config.yaml` (+ gitignore block unless tracked) when absent; add `--no-init` (or honour `SHATTER_NO_INIT=1`) for read-only use; status lines on stderr (p2-27a).
2. **Never write project state unless `.shatter/` exists or `--init` is passed**: first-run ergonomics regress (user must run `shatter init` first); QUICKSTART updated.
3. Prompt when interactive, skip when not (adds TTY detection; agents get option 2 behavior).

## Acceptance checks
- Decision recorded in SPEC §2.1/§2.8 and PROJECT-LAYOUT; the chosen flag/env implemented with a test that a fresh directory is untouched when opted out.

## Scope
In: policy + one flag. Out: init's own behavior.

## Size
small.

## Provenance
Audit 2026-09-04, section 11, action item 27; evidence audits/2026-09-04/usability-ui.md §2; docs-accuracy.md P2-3.
