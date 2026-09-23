---
slug: downstream-coverage-goals-epic
kind: new
title: "Track the downstream ≥90% coverage goals (kapow, zolem, pickpackit) in the tracker; stalled at 18-28% since 2026-07-07"
priority: P2
type: epic
labels: [agents, coverage, downstream, kapow, zolem, pickpackit, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Track the downstream ≥90% coverage goals (kapow, zolem, pickpackit) in the tracker; stalled at 18-28% since 2026-07-07

## Problem

In early July, Shatter set real-world success goals: at least 90% coverage on
kapow, zolem and pickpackit. These goals exist only in per-user agent memory
files. They were last measured on 2026-07-05..07, at 18-28%, and nothing
prompts anyone to re-measure or act. No tracker epic owns them.

## Evidence

- Memory files under `~/.claude/projects/-home-ketan-project-shatter/memory/`:
  - `project_pickpackit_coverage_goal.md`: metric, eligible set, baseline
    26.3% combined, 2026-07-07.
  - `project_kapow_shatter_advise_log.md`: 18.3%; the resolver dir is 18.8%
    (07-06).
  - `project_zolem_shatter_advise_log.md`: 28.0% lines, 2026-07-05.
- Newest dated entries: `~/project/zolem/docs/shatter/CHANGELOG-for-advise.md`
  2026-07-05 and `~/project/pickpackit/docs/shatter/CHANGELOG-for-advise.md`
  2026-07-07. `ls` re-checked 2026-09-23: pickpackit's file is dated Jul 7.
  kapow has no `docs/shatter/`.
- Open levers named in memory: str-j49xg (P1 epic, open), str-la75, str-jyxr,
  str-4yc9w, str-wfd2, str-bh9wu.
- This audit found the coverage metric itself inconsistent (Go scan inflates
  to 100%, Rust deflates to about 54%). It also found that explore resume
  ignores explorer mode. Any re-measurement must wait for those fixes:
  go-scan-coverage-clamp, rust-instrumentable-line-count and
  explore-resume-options-key (bucket shatter-artifacts-correctness).
- Finding: goals-09 (the verifier corrected P1 to P2: stalled tracking and
  ownership, not broken behavior). Source draft:
  `drafts/shatter-agent/27-downstream-coverage-goals-epic.md`.

## Acceptance criteria

- [ ] An in-repo doc (for example `docs/goals/downstream-coverage.md`)
      records, for each project, the metric definition, the eligible set, the
      run recipe and the dated measurements.
- [ ] This epic has one child per downstream project. Each child holds the
      last measurement, and the open levers are linked as dependencies.
- [ ] A re-measurement child is blocked by go-scan-coverage-clamp,
      rust-instrumentable-line-count and explore-resume-options-key. Its
      result is recorded in the doc with a date.
- [ ] The three memory files are reduced to pointers to the doc and the epic.

## Suggested approach

Start by copying the metric and recipe from the pickpackit memory file, which
is the most complete, into the doc. Then create the per-project children.
The concolic benchmark (concolic-vs-default-benchmark) uses one downstream
project; share its run recipe where possible.

## Out of scope

- Engine improvements themselves.

## Priority / type / labels

P2, epic. Labels: agents, coverage, downstream, kapow, zolem, pickpackit,
audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: none. Only the re-measurement child is blocked, as listed
  above.
- Related: concolic-vs-default-benchmark, str-j49xg.
