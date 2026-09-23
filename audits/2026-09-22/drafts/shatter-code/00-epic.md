# Epic: Audit 2026-09-22 findings — code, design and goals (shatter)

| field | value |
|---|---|
| action | new epic |
| type | epic |
| priority | P1 |
| labels | audit,epic |

<!-- body -->
## Purpose

Umbrella for tracker issues filed from the 2026-09-22 full-project audit, limited to
**code quality (L1), design (L4) and goal achievement (L5)** findings whose target is the
shatter repo. Documentation, UI and agent-system findings are filed separately.

Evidence lives under `audits/2026-09-22/` (gate logs, area reports, run transcripts);
it becomes available on main when the audit report lands.

## Suggested waves

1. **Make gates real** (everything else depends on trustworthy gates): the task-checksum
   poisoning fix, then the CI guard/triage, Task sources coverage, pre-commit hook.
2. **User-visible correctness P1s**: Z3 sort split, path under-count, setup ignored under
   concolic, resume keyed on options, explore JSON bundles, mixed-language scan, diff
   producer, revalidate outputs, TS flow map + switch/ternary, Go runtime module /
   go-tool path / rune literals, Rust crate-bridge stdout, coverage metric, release workflow.
3. P2 design/engine work, then P3 hygiene.

## Not in scope

Existing open issues confirmed still true by this audit are *not* duplicated here
(str-qwua7.5, .11, .13, .29, .30, .31, .39, .47, str-6nul9); see the audit index.
