# Replace removed `bd sync` in agent docs and refresh the frozen .beads/issues.jsonl snapshot (plus a freshness check)

- Priority: P1
- Type: bug
- Labels: agents,beads,docs,drift
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: new (partially covered by str-ly5bz; related L2: agent-repo-06)
- Source findings: prior-03, docs-16
- Parent: 01 (epic)
- Blocked by: none
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
## Problem
AGENTS.md mandates `bd sync` (including landing step 5), but the installed
bd 1.1.0 has no `sync` command. The committed `.beads/issues.jsonl` has not
been exported since 2026-09-07, yet CI drift-patrol tracker-hygiene and
`scripts/cleanup-merged-remote-branches.sh` (in-progress branch protection)
read that file, so both act on two-week-old tracker state.

## Current Code Facts
- `bd version` -> 1.1.0; `bd sync --help` -> `Error: unknown command "sync"`.
- `bd sync` is required at AGENTS.md:125, :293 and :340-369 (Beads Sync
  Cadence), and at `.claude/skills/audit/SKILL.md:385`. `.beads/PRIME.md:8`
  says `bd dolt pull → git commit` instead.
- `git log -1 -- .beads/issues.jsonl` -> 134dd616 (2026-09-07), 1,733 lines;
  the live DB has ~1,775 issues. At least 19 status mismatches (e.g.
  str-qwua7.4, .7, .8, .9, .15 show `open` in the JSONL but are closed).
- `docs/DRIFT-PATROL.md:54-55` states CI reads the committed export because bd
  is not installed in CI.
- `scripts/cleanup-merged-remote-branches.sh:27,105` read in-progress state
  from the tracked JSONL.
- `.beads/issues.recovered.jsonl` is tracked with no explanation.
- Two bd binaries are on PATH (`~/.local/bin`, `/usr/local/bin`); bd warns.

## Acceptance Criteria
- A decision recorded in this issue: keep `.beads/issues.jsonl` tracked or
  untrack it.
- If kept: every `bd sync` mention in AGENTS.md, `.beads/PRIME.md` and repo
  skills is replaced by the bd-1.x export command (e.g.
  `bd export -o .beads/issues.jsonl` + commit), stated in exactly one place and
  referenced elsewhere; the landing flow runs it; the JSONL is re-exported now.
- If untracked: cleanup script reads live bd (with a staleness warning) and CI
  drift-patrol tracker-hygiene degrades to SKIP instead of reading stale data.
- New drift-patrol check FAILs when the committed JSONL's newest `updated_at`
  is more than 3 days older than the newest closed issue in live bd (SKIP when
  bd is unavailable).
- A docs check (in `docs-smoke` or a meta test) runs `bd <subcommand> --help`
  for every bd subcommand named in AGENTS.md/skills and fails on unknown ones.
- `.beads/issues.recovered.jsonl` is deleted or explained in AGENTS.md.
- AGENTS.md quick reference states the expected bd version.

## Out of Scope
Configuring a Dolt remote (bento-side procedure). Sync cadence debate in
str-ly5bz beyond what is needed here.
