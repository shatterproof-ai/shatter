---
slug: downstream-coverage-goals-epic
kind: new
title: "Track the downstream coverage goals (kapow and pickpackit ≥90% feature / ≥80% UI, zolem ≥90%) in the tracker with an owner; stalled at 18-28% since 2026-07-07"
priority: P2
type: epic
labels: [agents, coverage, downstream, kapow, zolem, pickpackit, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Track the downstream coverage goals (kapow and pickpackit ≥90% feature / ≥80% UI, zolem ≥90%) in the tracker with an owner; stalled at 18-28% since 2026-07-07

## Problem

In early July, Shatter set real-world success goals on three downstream
projects. They exist only in per-user agent memory files, which a fresh agent
on another machine cannot read. They were last measured on 2026-07-05..07, at
18-28%, and nothing prompts anyone to re-measure or act. No tracker epic and
no person owns them.

## The goals as set (quoted from the memory files, 2026-09-23)

| Project | Goal | Excluded | Eligible set | Metric | Last measurement |
|---|---|---|---|---|---|
| kapow (set 2026-07-01) | ≥90% line coverage of true feature code; **UI code may be 80%** | tests, generated code | `scripts/shatter-source-set.sh list-eligible` in kapow (1,015 files: 713 Go, 302 TS/TSX; 687 after the kapow-yrxv scope exclusions) | `shatter explore` summary.json line fields: `covered_completed_lines` vs `discovered_function_span_lines`, aggregated over the eligible set | 18.3% exec lines (Go 17.3%, TS 34.6%), 2026-07-06; resolver dir 18.8% |
| zolem (set 2026-07-03) | ≥90% line coverage of true feature code (no UI layer, so flat 90%) | tests, generated code | all non-test `*.go` under `cmd/` + `internal/` (64 files, about 9,780 lines) | whole-repo scan line coverage | 28.0% lines (1754/6265), branch 63.5%, 2026-07-05 |
| pickpackit (set 2026-07-02) | ≥90% line coverage of true feature code; **UI may be 80%** | tests, generated code, `*.d.ts`, `api/src/bin`, `test_support.rs`, `images/tests.rs` | 115 `web/src` TS/TSX + 60 `api/src` `.rs` files (2026-07-03; the file lists were kept only in a session scratchpad and must be regenerated) | scan/explore line coverage (covered vs instrumentable/function-span lines) over the eligible set, UI vs logic split | combined 26.3% lines (1957/7428): web 39.2%, api 21.4%, 2026-07-07 (durable summaries: pickpackit `docs/shatter/baselines/20260707-{web,api}-all-summary.json`) |

Sources: `~/.claude/projects/-home-ketan-project-shatter/memory/`
`project_kapow_shatter_advise_log.md`, `project_zolem_shatter_advise_log.md`,
`project_pickpackit_coverage_goal.md`; `~/project/zolem/docs/shatter/CHANGELOG-for-advise.md`
(newest entry 2026-07-05) and `~/project/pickpackit/docs/shatter/CHANGELOG-for-advise.md`
(2026-07-07). kapow has no `docs/shatter/`.

## Evidence

- Open levers named in memory: str-j49xg (P1 epic, open), str-la75,
  str-jyxr, str-4yc9w, str-wfd2, str-bh9wu (all open, 2026-09-23).
- This audit found the coverage metric itself inconsistent (Go scan inflates
  to 100%, Rust deflates to about 54%) and that explore resume ignores
  explorer mode. Re-measurement must wait for go-scan-coverage-clamp,
  rust-instrumentable-line-count and explore-resume-options-key (bucket
  shatter-artifacts-correctness).
- Finding: goals-09 (the verifier corrected P1 to P2: stalled tracking and
  ownership, not broken behavior). Source draft:
  `drafts/shatter-agent/27-downstream-coverage-goals-epic.md`.

## Acceptance criteria

- [ ] **Owner.** Before any child is started, the maintainer names the epic's
      owner (`bd update <epic> --assignee <name>`). An unowned epic is not
      "done" with any other criterion.
- [ ] **Durable goal doc.** `docs/goals/downstream-coverage.md` (landed via
      launch-work/land-work) records, per project: the goal with its UI
      threshold (80% for kapow and pickpackit UI code, flat 90% for zolem),
      how "UI code" is classified, the exclusions, the command that produces
      the eligible set (committed in the downstream repo or in this doc, so it
      does not depend on a scratchpad), the metric formula, the exact run
      recipe (shatter binary path, cache-clearing prerequisites such as
      kapow's `~/.config/shatter/go-workspace/analysis` note, timeouts), and
      the dated measurements above.
- [ ] **One child per project**, each with its own completion condition: the
      child closes when a measurement recorded in the doc meets that project's
      goal (feature ≥90% and UI ≥80% where applicable), or when the maintainer
      changes the goal in the doc. The open levers are linked to the relevant
      child as dependencies.
- [ ] **Re-measurement child**, blocked by go-scan-coverage-clamp,
      rust-instrumentable-line-count and explore-resume-options-key. It
      re-runs the recipe for all three projects and records dated results in
      the doc (with the shatter SHA used).
- [ ] The three memory files are reduced to pointers to the doc and the epic.

## Suggested approach

Start from the pickpackit memory file, which is the most complete. The
concolic benchmark (concolic-vs-default-benchmark) uses one downstream
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
