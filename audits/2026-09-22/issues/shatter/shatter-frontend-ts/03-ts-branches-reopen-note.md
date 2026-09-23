---
slug: ts-branches-reopen-note
kind: reopen-note
title: "Reopen-note on closed str-wsg: switch/ternary/value-position &&,|| are analyzed but never instrumented"
priority: P1
type: note
labels: [typescript, instrumentation, audit]
parent_epic: ""
blocked_by: []
existing_id: str-wsg
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Reopen-note on closed str-wsg

**Target:** `str-wsg` ("Extract branches from TypeScript AST in analyzer", P1, CLOSED).
**Action:** add a comment only (`bd comments add str-wsg`). Do **not** reopen.
**Ordering:** file after `ts-switch-ternary-instrumentation` so the comment can cite its real id. Replace `<ts-switch-ternary-instrumentation id>` below before posting.

## Comment text

> **Audit 2026-09-22 note.** This work added `switch`, `ternary` and `logical_and`/`logical_or` branch extraction to the TS **analyzer** only (`shatter-ts/src/analyzer.ts:1364-1471`). The instrumentor was never extended:
>
> - `switch` gets line records only (`instrumentor.ts:1393-1411`);
> - there is no `ConditionalExpression` handling;
> - value-position `&&`/`||` get no probe.
>
> Analyze and instrument also number branches independently. The core joins them by id (`coverage_metrics.rs:477 extract_targets_inner`), so coverage and "Uncovered" hints are attributed to the wrong branches. Example: for a ternary followed by an `if`, runtime id 0 is the `if` but analysis id 0 is the ternary. At HEAD, `h(x) = x > 1 ? 1 : 0` reports `0/1 branches` alongside `100% coverage (1/1 lines)`. Closed str-w0d.1 claimed constraint emission for these branch types, and that emission does not exist.
>
> The fix is tracked in <ts-switch-ternary-instrumentation id>. It covers instrumenting switch case labels, ternaries and value-position `&&`/`||` chains with defined decision semantics (fallthrough and `default` record no extra decision), one shared branch enumerator so analyze and instrument ids cannot drift (including same-line branches), an analyze/instrument id-alignment test, and per-BranchType known-answer E2E fixtures. Evidence: audit 2026-09-22 findings frontend-ts-02 and prior-17 (`audits/2026-09-22/areas/frontend-ts.md` F2).
