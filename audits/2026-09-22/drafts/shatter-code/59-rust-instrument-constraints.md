# Rust instrumentation emits invalid or stringly-typed constraints: hand-rolled JSON breaks on `1.` floats and control chars, and match arms become string equality on pattern text

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P2 |
| labels | rust-frontend,instrumentation,solver,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-qwua7.36, str-mambd |
| source findings | frontend-rust-02, frontend-rust-03 |

<!-- body -->
## Problem

Runtime branch constraints in shatter-rust are hand-formatted JSON. Float literals like `1.` and strings with control characters produce invalid JSON, which the runtime silently turns into `Unknown`. Match arms are encoded as `{param n} == {const str "<pattern source>"}` ("0", "1 ..= 5", "k", "_"), and the analyzer encodes a guarded binding arm as `n == "k"`. The solver cannot use any of these. str-qwua7.36 covers moving to serde SymExpr but not these cases.

## Current code facts / evidence

- `shatter-rust/src/instrument.rs:671-712` `constraint_for_lit` uses `f.base10_digits()`; `escape_json_string` handles only \\, \", \n, \r, \t.
- `shatter-rust-runtime/src/lib.rs:180-186` falls back to `SymConstraint::Unknown` without warning.
- `shatter-rust/src/instrument.rs:413-446` match-arm encoding; `shatter-rust/src/analyzer.rs:1753-1771` binding+guard arm; `shatter-rust/CLAUDE.md:83` describes the core's token-text workaround (str-mambd).
- Concolic probe: `if s == "a\u{1}b"` never reached in 60 iters; `clean(n)` (n+1>5 then a 4-arm int match) used n=0 for all 60 iterations and reached 3 of 7 returns while batch line said 5/7 branches.

## Acceptance criteria

- instrument.rs builds `protocol::SymExpr` and serializes with serde_json (folds str-qwua7.36).
- Match arms use the analyzer's typed pattern conversion: int literal eq, range and(ge,le), binding → true or guard expr, wildcard → negation of earlier arms; analyzer binding+guard arm fixed.
- branch_hit warns/counts when constraint parsing fails; proptest: every instrumented constraint string parses.
- E2E known-answer fixture with an int match needing Z3 (no minable literals) reaches every arm; core token-text workaround retired afterwards.

## Suggested approach

Land with str-qwua7.36 (append a note there linking this issue).

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: frontend-rust-02, frontend-rust-03 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-qwua7.36, str-mambd
