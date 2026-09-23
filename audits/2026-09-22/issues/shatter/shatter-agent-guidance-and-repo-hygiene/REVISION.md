# REVISION: shatter-agent-guidance-and-repo-hygiene (2026-09-23)

Primary review: `../../crosscheck/shatter-agent-guidance-and-repo-hygiene.codex.md` (Codex).
Secondary review: `../../crosscheck/shatter-agent-guidance-and-repo-hygiene.md` (degraded same-runtime).
Tracker state checked live with `bd show` on 2026-09-23: str-u394l.4, str-qwua7.44, .51, .52, .53, .14 (closed).
Nothing is filed (D6).

## Codex findings

| # | Sev | Finding (one line) | Action | Files |
|---|---|---|---|---|
| C1 | MAJOR | 01: a config snapshot cannot cover fixtures that are never executed (not in `ENTRYPOINTS`) | applied: new AC6 registration-completeness test (tracked identity-writing files must be registered or allowlisted with a reason) plus AC7 for the four unregistered Rust sites (two more found: `init.rs:379`, `generated_paths.rs:750`); the guard's claim is narrowed if any are allowlisted | 01 |
| C2 | MAJOR | 02: "SKIP when not in a git work tree" swallows `core.bare=true` | applied: explicit discovery precedence (`--git-dir`/`--git-common-dir`, read config, SKIP only if no git dir or CI) plus a test asserting `core.bare=true` gives FAIL, not SKIP | 02 |
| C3 | MAJOR | 03: actor override is `BEADS_ACTOR`, not `BD_ACTOR`; actor/assignee/owner not distinguished | applied: verified `bd --help` on 1.1.0; note now corrects str-qwua7.51's own `BD_ACTOR` plan, separates actor/assignee/owner, and gives a probe that records all four before dropping the identity half | 03 |
| C4 | MAJOR | 04: ancestor-of-origin/main rule forbids citing valid unmerged SHAs | applied: rule moved out of 04 into the str-qwua7.51 close-reason half (03 item 4) and scoped: only "landed" / "checked on main" claims need an ancestor SHA; other SHAs must be labelled | 03, 04 |
| C5 | MAJOR | 04/05/07: cleanup ACs have no outcome if deletion is declined | applied: 04 defines `retain` as a valid disposition; directory cleanup from 05 and 07 moved to new issue 13 where `retain` is a documented outcome | 04, 05, 07, 13 |
| C6 | MAJOR | 06: `task <ns>:test` is not governed (e.g. `core:test` runs cargo directly) | applied: verified `shatter-core/Taskfile.yml:14-30` and `gate-wrapper.sh` usage; 06 now requires governed invocations (`task affected` / `task check` / `gate-wrapper.sh <label> task <ns>:test`) and proof via `gate-times.csv` rows | 06 |
| C7 | MAJOR | 06: `bd <cmd> --help` oracle accepts `bd epic list` (exit 0) | applied: verified exit 0; lint moved to str-u394l.4 note (14) with command-tree resolution against "Available Commands"; 06 also fixes the `bd epic list` still at audit `SKILL.md:115` | 06, 14 |
| C8 | MAJOR | 06/07/10: overlap with str-u394l.4, .52/.53, .44 not reconciled | applied: verified live. 06 lint -> note 14 on str-u394l.4; 07 converted to a note on str-qwua7.53 plus new note 15 on str-qwua7.52; 10 now references .44's banner convention, drops its own banner AC and posts a companion comment on .44; 10's str-qwua7.43 comment dropped (already covered by `qwua7-43-bench-dev-dep-cycle` in the concolic bucket) | 06, 07, 10, 14, 15 |
| C9 | MAJOR | 09: stories-impact-check placed after the change | applied: AC1 is a pre-edit rule in CLAUDE.md Agent Workflow; completion only verifies it ran | 09 |
| C10 | MAJOR | 09: "SPEC.md touched" is not proof; exit codes live outside `args.rs` | applied: trigger-path list includes `main.rs`, `helpers.rs`, `render.rs`, `commands/**`, core report renderers; semantic check requires a new §8 row, a `Last updated` bump, and a hunk in each §2 subsection the row names; unit tests per failure mode | 09 |
| C11 | MAJOR | 11: `gate_scope: task affected` violates the emit-commands contract | applied: verified bento `landing-config.md`; note now requires an adapter script emitting `task <gate>` lines and behavioural tests of its output | 11 |
| C12 | MINOR | 11: Claude-only JSON leaves Codex unconfigured | applied: verified `swarm-discover.py:18-22,257-270`; config goes at root `swarm-config.json`, validated with `--runtime claude` and `--runtime codex` | 11 |
| C13 | MINOR | 04: `e50fc399` deletes, not restores, `.claude/agents` | applied: verified three `D` entries; corrected | 04 |

## Secondary (degraded) review findings

| Sev | Finding | Action | Files |
|---|---|---|---|
| MAJOR | 01: whole-file `.git/config` hash flakes under concurrent sessions | applied: guarded key set (`user.*`, `core.bare`, `core.hooksPath`, `core.worktree`, new example-domain values) plus a concurrency no-false-positive test | 01 |
| MINOR | 01: probe must use a parameterised function | applied (AC5 requires it) | 01 |
| MINOR | 01: "every commit" overstated | applied ("nearly every", ~6 real commits) | 01 |
| MINOR | 02: check should inspect the invoking checkout | applied | 02 |
| MINOR | 03: root cause asserted then hedged | applied ("probable", confirmed by the probe) | 03 |
| MINOR | 04: bundles three tasks | applied: str-qwua7.14 re-verify split to reopen-note 12; SHA rule to 03 | 04, 12 |
| MINOR | 04: "diagnosis ran against a corrupted tree" unproven | applied ("may have") | 04, 12 |
| MINOR | 05: `.codex/` symlinks will surface once un-ignored | applied: `/.codex/*` ignored, only `!/.codex/AGENTS.md` tracked; test cases for the symlinks | 05 |
| MINOR | 06: scope too wide; split the lint | applied (lint -> 14) | 06, 14 |
| MINOR | 06: cross-bucket `blocked_by` slug | kept: the filer resolves cross-bucket slugs; the block is limited to AC6 in the Dependencies text | 06 |
| MINOR | 08: merged-branch counts drifted | applied (37 of 64 incl. `origin/HEAD`, re-run before acting) | 08 |
| MINOR | 09: waiver not machine-readable | applied (`Spec-Waiver:` commit trailer) | 09 |
| MINOR | 10: dev-dependency framing overstated | applied (edge pre-existed since 4db63be3; the conflict is the second consumer) | 10 |

## Splits and conversions

- **04 `fixture-corruption-incident-reverify`** (slug kept): now the incident record plus a six-branch disposition table. Split out:
  - **12 `qwua7-14-reverify-on-main`** (new, `reopen-note` on str-qwua7.14): re-verification on an origin/main build. The filer only comments; `bd reopen` is manual.
  - The close-reason SHA rule went into note 03 (str-qwua7.51).
- **05 `agent-config-gitignore`**: str-umw3 orphan removal moved to 13.
- **06 `repo-skills-rot`** (slug kept): skill repair only. Lint requirements moved to **14 `u394l-4-skill-command-lint`** (new note on str-u394l.4).
- **07 `env-doctor-decisions`**: converted from `new` to `note-to-existing` on **str-qwua7.53** (slug kept). Storystore half moved to **15 `qwua7-52-storystore-interim-nudge`** (new note on str-qwua7.52). Orphan dirs moved to 13.
- **13 `orphan-worktree-dirs-cleanup`** (new, P3): the five doctor-flagged dirs plus `.claude/worktrees/str-umw3`, with `retain` as a valid outcome.
- **10**: banner-on-plan AC moved to a companion comment on str-qwua7.44; str-qwua7.43 comment AC dropped (covered by `qwua7-43-bench-dev-dep-cycle`).

## Cross-bucket effects

- `shatter-tracker-and-beads/08-tracker-reconciliation-sweep.md:91` lists "Re-verifying str-qwua7.14 (fixture-corruption-incident-reverify)"; that work now lives in `qwua7-14-reverify-on-main`.
- `env-doctor-decisions` is now a note on str-qwua7.53, not a new issue; MANIFEST.md rows for it (`:324`, `:507`, `:657`, `:854`) still say `new`.
- The concolic bucket's `qwua7-43-bench-dev-dep-cycle` stays the only owner of the bench-file scope addition on str-qwua7.43.
