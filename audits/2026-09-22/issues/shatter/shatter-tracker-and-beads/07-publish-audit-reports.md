---
slug: publish-audit-reports
kind: new
title: "Land the 2026-09-22 audit report and evidence on main before the bulk issue filer runs (filing bootstrap)"
priority: P1
type: task
labels: [agents, audit, docs, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Land the 2026-09-22 audit report and evidence on main before the bulk issue filer runs (filing bootstrap)

## Problem

Every issue drafted from the 2026-09-22 audit cites evidence under
`audits/2026-09-22/` (`findings.json`, `areas/`, `gates/`, `sessions/`). That
evidence exists only on the local, unpushed branch `audit-2026-09-22`. If the
issues are filed first, a fresh agent that picks one up cannot read its
evidence. Past audits were lost this way: the 2026-07-10 report was never
committed (str-sff87), and the 2026-09-04 and 2026-06-09 audit branches have
been deleted (their recovery is audit-2026-09-04-report-recovery).

This issue is the **bootstrap** step of filing. It must not depend on any
other unfiled audit issue. In particular it needs no Dolt-remote procedure:
landing a report uses land-work exactly as it works today, and makes no
tracker-sync step.

## Filing bootstrap (for the maintainer; D6: agents file nothing)

1. **Now, before any filing, no issue needed:** preserve the unreachable audit
   commits in `/home/ketan/project/shatter` so `git gc` cannot drop them:
   `git branch audit-2026-09-04-recovered e067979d` and, if wanted,
   `git branch audit-2026-06-09-recovered f9dad247`.
2. **File only this issue** (and the shatter audit epic): run the filer with
   `ONLY=shatter` and `HOLD` set to every other shatter slug, so the ledger
   records `epic:shatter` and `publish-audit-reports`. (Or create it by hand
   and add its ledger row, so the bulk run skips it.)
3. **Work this issue** to completion (below).
4. **Run the bulk filer** for everything else. Its evidence paths now resolve
   on origin/main.

## Evidence (re-verified 2026-09-23)

- `git ls-remote --heads origin 'audit*'` returns only
  `audit-kapow-2026-05-24`; `audit-2026-09-22` is unpushed. On 2026-09-23 it
  had 5 commits over origin/main (first 56c86168) and 850 tracked files under
  `audits/2026-09-22/`, of which 16 are session-transcript files under
  `audits/2026-09-22/sessions/` and 343 are issue drafts under
  `audits/2026-09-22/issues/`. The untracked `goals-runs/` is about 255 MB.
- `git ls-tree origin/main -- audits/` lists only `2026-02-28.md`,
  `2026-05-21.md` and `kapow-2026-05-21`.
- str-qwua7.44 (open, P2, docs IA) decides that root `audits/` merges into
  `docs/audits/` **after** the in-flight audit branch lands at `audits/`, and
  that `docs/INDEX.md` gains an "Audits" section. So the landing path is
  `audits/2026-09-22.md` + `audits/2026-09-22/`, and the INDEX section stays
  with .44.
- Findings: agent-repo-04 (verified P1), docs-04. Source draft:
  `drafts/shatter-agent/06-publish-audit-reports-and-audit-skill-landing.md`.

## Acceptance criteria

- [ ] **Privacy review done first.** The maintainer reviews the 16
      `audits/2026-09-22/sessions/` files (and any other transcript excerpt)
      for content that should not be on a public main; each file is kept,
      redacted or dropped, and the list with the decision is in the close
      reason.
- [ ] `audits/2026-09-22.md` and the tracked `audits/2026-09-22/` evidence
      land on origin/main through launch-work/land-work. `goals-runs/` stays
      out. Whether `audits/2026-09-22/issues/` (drafts, filer, ledger) lands
      too is the maintainer's call, recorded in the close reason.
- [ ] **Evidence paths resolve.** A script extracts every
      `audits/2026-09-22/...` path cited in `audits/2026-09-22/issues/**/*.md`
      and runs `git cat-file -e origin/main:<path>` on each (fragments like
      `:NN` stripped). The output shows 0 missing paths and is in the close
      reason. The script is kept (for example under `scripts/`) so the audit
      skill can reuse it.
- [ ] The landing does not run or document `bd sync` and does not commit
      `.beads/*.jsonl`.
- [ ] This issue is closed **before** the bulk filer run; the close reason
      gives the origin/main SHA that contains the report.

## Out of scope

- Recovering and landing the 2026-09-04 report
  (audit-2026-09-04-report-recovery).
- Why the audit branches were deleted (audit-branch-deletion-cause).
- The `/audit` skill's post-audit order (str-qwua7.22; see
  qwua7-22-audit-land-before-file-note).
- The `docs/INDEX.md` "Audits" section and any move to `docs/audits/`
  (str-qwua7.44).
- Filing the 2026-09-22 issues (D6: the maintainer's filer).

## Priority / type / labels

P1, task. Labels: agents, audit, docs, audit-2026-09-22.

## Parent epic

Epic: Audit 2026-09-22 findings (shatter).

## Dependencies

- Blocked by: none. It is filed and closed before the bulk filer run.
- Related: str-qwua7.44, str-qwua7.22, str-sff87 (closed),
  audit-2026-09-04-report-recovery.
