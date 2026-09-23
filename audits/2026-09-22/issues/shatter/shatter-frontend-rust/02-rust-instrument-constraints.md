---
slug: rust-instrument-constraints
kind: new
title: "Rust instrumentation emits invalid or stringly-typed branch constraints: hand-rolled JSON breaks on `1.` floats and control chars; match arms become string equality on pattern text"
priority: P2
type: bug
labels: [rust-frontend, instrumentation, solver, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Rust instrumentation emits invalid or stringly-typed branch constraints: hand-rolled JSON breaks on `1.` floats and control chars; match arms become string equality on pattern text

## Problem

shatter-rust's instrumentor builds runtime branch constraints by formatting JSON strings by hand. Two defects follow from that, and Z3 cannot use the constraints in either case.

1. **Invalid JSON.** A float literal written `1.` is emitted as `"value":1.`, which is not valid JSON. Strings containing control characters other than `\n`, `\r`, `\t` (e.g. U+0001) are embedded raw. The runtime cannot parse either constraint and silently records `SymConstraint::Unknown`, with no warning and no counter.
2. **Match arms as string equality.** Every match arm is encoded as `eq(param "<scrutinee token text>", const str "<pattern token text>")`. So for `match n { 0 => .., 1..=5 => .., k if k > 100 => .., _ => .. }` with `n: i64`, the constraints are `n == "0"`, `n == "1 ..= 5"`, `n == "k"` and `n == "_"`. The analyzer's static builder is also wrong for binding arms: it encodes `k if k > 100` as `n == "k"` (a string constant) and ignores the guard.

The core works around part of this for enums by matching raw token text alphanumerically (str-mambd). That workaround does not help integer ranges, bindings, guards or wildcards.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-rust/src/instrument.rs:671-702` `constraint_for_lit`: float literals use `f.base10_digits()` raw (`:682`). `instrument.rs:704` `escape_json_string` escapes only `\\`, `"`, `\n`, `\r`, `\t`.
- `shatter-rust-runtime/src/lib.rs:178-186`: an unparseable constraint falls back to `SymConstraint::Unknown { hint }` with no warning.
- `shatter-rust/src/instrument.rs:413-446` `visit_expr_match_mut`: builds `{"kind":"bin_op","op":"eq","left":{"kind":"param","name":"<match expr tokens>"},"right":{"kind":"const","type":"str","value":"<pattern tokens>"}}` for every arm, including bindings and wildcards. The param node has no `path` field.
- `shatter-rust/src/analyzer.rs:1750-1771` walks match arms without looking at `arm.guard`. `analyzer.rs:2128-2137` (`build_pattern_sym_expr`, `syn::Pat::Ident`) produces `scrutinee == Const(Str(ident))` for a binding.
- `shatter-rust/CLAUDE.md:81-83` describes the core-side token-text workaround (str-mambd).
- Audit probes (findings frontend-rust-02/03, not re-run): an explore artifact listed `x > 1.` and `s == "a\u{1}b"` constraints as kind `unknown`, and concolic explore never reached `if s == "a\u{1}b"` in 60 iterations. For `clean(n)` (`n + 1 > 5`, then the 4-arm int match above), all 60 concolic iterations used n=0 and reached 3 of 7 returns, while the batch line reported "6 paths, 5/7 branches". The analyzer's typed builder emits `1.0` and `"a\u0001b"` correctly for the same source.

## Acceptance criteria

- [ ] `instrument.rs` builds `protocol::SymExpr` values and serializes them with `serde_json`, with no hand-formatted constraint JSON left. This is the approach of open **str-qwua7.36** (typed SymExpr). Land it with or on top of that issue.
- [ ] Match arms use typed pattern conversion shared with the analyzer. Int literal → `eq(int)`. Range → `and(ge, le)` (respecting `..` vs `..=`). Binding → `true`, or the guard expression with the binding substituted by the scrutinee. Wildcard → negation of the earlier arms. Guards are conjoined on every arm that has one.
- [ ] The analyzer's binding+guard arm encoding is fixed in the same way (no `Const(Str(ident))` for bindings).
- [ ] When `branch_hit` cannot parse a constraint, it emits a warning or increments a counter that shows up in the execute result/telemetry. Silent `Unknown` is gone.
- [ ] Proptest: every constraint string the instrumentor emits parses as a `SymConstraint` that is not `Unknown`, for generated literals (floats incl. `1.`, `1e10`, strings with arbitrary chars) and generated match patterns.
- [ ] E2E known-answer fixture in `shatter-core/tests/e2e_concolic_rust.rs`: an integer match whose arms need Z3 (no literal that constant mining could find), with a range, a guarded binding and a wildcard. Show `cargo test --test e2e_concolic_rust <name>` failing before the change and passing after, and include both outputs in the close note.
- [ ] After the above is green, retire the core token-text workaround described in `shatter-rust/CLAUDE.md:81-83` (or record in the close note why part of it must stay for cross-file `rename_all` enum values), and update that CLAUDE.md section.

## Suggested approach

Do the JSON half through str-qwua7.36: make `instrument.rs` call the analyzer's `build_sym_expr` / `build_pattern_sym_expr` and embed `serde_json::to_string(&expr)` as the `branch_hit` argument. Then extend `build_pattern_sym_expr` for ranges, bindings, guards and wildcards, so the static (analyzer) and runtime (instrument) paths share one encoder. That keeps the parallel-parity rule in the project CLAUDE.md. Post a comment on str-qwua7.36 linking this issue when filing.

## Out of scope

- Other parts of str-qwua7.36's scope (let patterns, arithmetic, bitwise, calls) beyond what the shared encoder needs.
- TS and Go instrumentors.

## Size

M

## References

- Findings frontend-rust-02, frontend-rust-03 (audit 2026-09-22). Old draft: `drafts/shatter-code/59-rust-instrument-constraints.md`.
- Related: str-qwua7.36 (open, typed SymExpr: the approach for the JSON half), str-mambd (closed, enum-domain token-text workaround).
