# Landing evidence must cover each changed language: verifier runs no TS/Go/rust-fe tests and hides output

- Priority: P2
- Type: task
- Labels: agents,landing,quality-gates
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (partially covered by str-qwua7.55, str-35vtk.24, str-qwua7.2; append note to str-qwua7.55)
- Source findings: tests-ci-06 (duplicate-open sibling: agent-repo-11 -> str-qwua7.55)
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
The land-work verifier gates landings with gates that do not cover the changed
code, and it hides their output. `/pre-completion` has no row checking that
the evidence covers each changed language, and no PBT row despite CLAUDE.md
checklist item 2. Together with the Task checksum problem (cached leaves report
"is up to date"), landings can be green without the relevant tests running.

## Current Code Facts
- `scripts/land_work_verifier.sh` (unchanged since d01a22db, 2026-08-05) runs
  `task test-standard`, `task parity`, `task conformance`; line 17 sends each
  check's output to `/dev/null 2>&1`; emits only `{name,status}`, no
  `executed`/`wall_seconds`; header claims "Runs the same gates ci.yml uses"
  (CI runs `task check` + shatter-llm).
- `test-standard` (Taskfile.yml:103-111) = frontends-built + workspace clippy +
  `cargo test --workspace` (core/cli/llm); no ts, go or rust-fe unit tests.
- `.agent-plugins/bento/bento/land-work/verifier.json`: no timeout.
- `.claude/skills/pre-completion/SKILL.md`: no property-test row, no mention
  that "is up to date" means not executed.
- bento already supports per-check `executed` (bento-rdtn.6) and log
  persistence (bento-rdtn.4); shatter has not adopted them.

## Acceptance Criteria
- `/pre-completion` adds rows: (1) gate evidence covers every language/crate in
  the diff (list: rust core/cli, shatter-ts, shatter-go, shatter-rust,
  shatter-rust-runtime, shatter-llm → required task), (2) PBT adequacy per
  CLAUDE.md item 2, (3) any "Task ... is up to date" line for a required leaf
  means re-run with `--force` or a fresh `TASK_TEMP_DIR`.
- Verifier tees each check's output, emits `executed` (parsed from Task's
  "is up to date" lines) and `wall_seconds` per check, and runs `task affected`
  for the landing diff (or the header/CLAUDE.md claim is corrected).
- `verifier.json` has a timeout.
- Note appended to str-qwua7.55 linking this issue.

## Out of Scope
Root-cause fix for Task checksum poisoning (product/gates issue under str-qwua7.3).
