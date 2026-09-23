---
slug: qwua7-31-eslint-evidence
kind: note-to-existing
title: "Note on str-qwua7.31: typescript-eslint finds 72 production issues in shatter-ts, including dead code and a default export"
priority: P2
type: note
labels: [typescript, lint, audit]
parent_epic: ""
blocked_by: []
existing_id: str-qwua7.31
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.31

**Target:** `str-qwua7.31` ("Add ESLint to shatter-ts with the rules ts-conventions already claims are enforced", P2, OPEN).
**Action:** post one comment (`bd comments add str-qwua7.31`). Keep P2. The comment corrects the title's premise but does not rename the issue; the maintainer may choose to.

## Comment text

> **Audit 2026-09-22 note** (finding frontend-ts-12, partially confirmed; evidence in `audits/2026-09-22/areas/frontend-ts.md` F12).
>
> **Still true:** there is no ESLint in `shatter-ts/package.json` or `shatter-ts/Taskfile.yml`.
>
> **Premise correction (verifier):** the ts-conventions skill says *prefer* ESLint. It does not claim that lint is enforced today. The title's "already claims are enforced" overstates this. The work is still warranted.
>
> **New, quantified evidence.** typescript-eslint 8 `recommendedTypeChecked`, plus the rules the skill names, was run on a scratch copy over production files only (tests excluded). It reported **72 findings** in about 16.7k lines:
>
> | Rule | Count |
> |---|---|
> | no-unnecessary-type-assertion | 23 |
> | no-unused-vars | 14 |
> | no-unsafe-call | 7 |
> | no-unsafe-return | 6 |
> | unbound-method | 5 |
> | consistent-type-imports | 5 |
> | no-unsafe-member-access | 4 |
> | no-unsafe-assignment | 3 |
> | no-base-to-string | 2 |
> | no-restricted-exports (default export) | 1 |
>
> Test files add 77 more (19 no-unsafe-assignment, 6 no-require-imports, 3 no-implied-eval). The verifier did not re-run ESLint, so treat the counts as approximate. The examples below were re-checked in code at 56c86168:
> - `shatter-ts/src/executor.ts:132`: `SUBPROCESS_SYMBOLS` is dead (declared, never read; unused since str-3ky9.12, 2026-03-11).
> - `shatter-ts/src/logger.ts:22`: a default export, against the skill's "no default exports".
> - `shatter-ts/src/analyzer.ts:1890` unused `checker`; `shatter-ts/src/instrumentor.ts:298` unused `sourceFile`.
> - `shatter-ts/src/executor.ts:2808`: `String(thrownError)` can yield `[object Object]` in connection-failure messages.
>
> Zero `no-explicit-any` and zero `ban-ts-comment` findings in production code, so adoption is cheap.
>
> **Proposed acceptance additions:**
> - Adopt `recommendedTypeChecked` plus the skill's named rules, and fix or explicitly disable (with a reason) each of the 72 production findings.
> - Wire `ts:lint` into `ts:test-fast` and `task check`. At close, show `task check` actually running the lint step, not a cached no-op.
> - Either align the ts-conventions skill wording with what is enforced, or retitle this issue.
