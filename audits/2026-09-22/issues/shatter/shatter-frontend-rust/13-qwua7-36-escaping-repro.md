---
slug: qwua7-36-escaping-repro
kind: note-to-existing
title: "NOTE on str-qwua7.36: audit repros for invalid hand-formatted constraint JSON (`1.` floats, control chars) and silent Unknown"
priority: P2
type: bug
labels: [rust-frontend, instrumentation, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.36
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# NOTE on str-qwua7.36: audit repros for invalid hand-formatted constraint JSON (`1.` floats, control chars) and silent Unknown

Target: **str-qwua7.36** (open, "build runtime constraints as typed protocol::SymExpr instead of hand-formatted JSON"). Action: post the comment below with `bd comments add str-qwua7.36 ...`. Do not change priority or scope ownership; str-qwua7.36 already owns the typed builder, serde serialization, deletion of `escape_json_string` and the proptest. The new match-arm semantics issue (`rust-instrument-constraints`) is blocked by str-qwua7.36.

## Comment text

> **Audit 2026-09-22 note** (finding frontend-rust-02). Two concrete repros for this issue's "second place to get escaping/shape wrong" point, plus one gap the current acceptance does not cover.
>
> **Repros (code re-verified at main 16794cef):**
> - `shatter-rust/src/instrument.rs:671-702` `constraint_for_lit`: float literals are emitted with `f.base10_digits()` raw (`:682`), so `if x > 1. {}` produces `"value":1.`, which is not valid JSON.
> - `instrument.rs:704` `escape_json_string` escapes only `\\`, `"`, `\n`, `\r`, `\t`, so `if s == "a\u{1}b" {}` embeds a raw U+0001 in the JSON string, which is invalid JSON.
> - Audit probe (not re-run): an explore artifact listed the `x > 1.` and `s == "a\u{1}b"` constraints as kind `unknown`, and concolic explore never reached `if s == "a\u{1}b"` in 60 iterations. The analyzer's typed builder emits `1.0` and `"a\u0001b"` correctly for the same source.
>
> **Please add to acceptance:**
> - The proptest over generated conditions includes float literals written `1.`, `1e10`, `1.5e-3`, and string literals with arbitrary chars including U+0000-U+001F. It deserializes the emitted string as `protocol::SymExpr` (the instrumentor emits bare `SymExpr`; `shatter-rust-runtime/src/lib.rs:178-186` `branch_hit` wraps it into `SymConstraint::Expr`), not as `SymConstraint`.
> - Silent `Unknown`: `branch_hit` (`shatter-rust-runtime/src/lib.rs:178-186`) turns any unparseable constraint into `SymConstraint::Unknown { hint }` with no warning or counter. Make that observable (a counter in the execute result / telemetry, or a warning on stderr that the frontend surfaces), with a unit test that feeds `{"value":1.}` and asserts the counter/warning. If you prefer to keep this issue narrow, say so here and the audit will file it separately.
> - E2E: an `e2e_concolic_rust` case that reaches `if s == "a\u{1}b"` via a runtime constraint. Note that plain `cargo test --test e2e_concolic_rust` runs nothing (all cases are `#[ignore]`d); use the `task e2e-rust-governed` command with `-- --include-ignored` and paste the `test result:` line.
