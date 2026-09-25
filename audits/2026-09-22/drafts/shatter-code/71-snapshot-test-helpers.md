# Snapshot tests self-create missing snapshots and pass; four copy-pasted helpers; whitespace-collapsed comparisons; no CLI output snapshots

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | task |
| priority | P2 |
| labels | testing,report,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | none |
| source findings | tests-ci-08 |

<!-- body -->
## Problem

Deleting a snapshot file makes its test pass (it rewrites and returns). Whitespace normalization hides layout regressions in markdown and HTML, and no test pins explore/scan terminal output or --help.

## Current code facts / evidence

- `shatter-core/tests/outcome_md_snapshots.rs:45-51`, `run_markdown_ordering_snapshots.rs:38-44`, `source_set_summary_snapshots.rs:32-38`: `if !path.exists() { write(actual); return; }`; `html_snapshots.rs:36` normalize_ws.
- 4 separate `fn assert_snapshot`; 8 snapshot files total in shatter-core/tests/snapshots; none in shatter-cli/tests.

## Acceptance criteria

- insta adopted (missing snapshot fails under CI=1); helpers deleted.
- Markdown compared byte-exact; HTML compared after structural normalization only.
- Normalized CLI snapshots (paths relativised, timings stripped) for explore/scan on 01-arithmetic TS/Go and top-level --help.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: tests-ci-08 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: none
