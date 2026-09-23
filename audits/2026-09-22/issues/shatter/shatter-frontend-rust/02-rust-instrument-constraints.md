---
slug: rust-instrument-constraints
kind: new
title: "Rust match arms are encoded as string equality on pattern text: ranges, bindings, guards and wildcards give Z3 nothing to solve"
priority: P2
type: bug
labels: [rust-frontend, instrumentation, solver, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [qwua7-36-escaping-repro]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Rust match arms are encoded as string equality on pattern text: ranges, bindings, guards and wildcards give Z3 nothing to solve

## Problem

Both the shatter-rust instrumentor (runtime constraints) and the analyzer (static branch conditions) encode each match arm as `eq(param "<scrutinee token text>", const str "<pattern token text>")`. For `match n { 0 => .., 1..=5 => .., k if k > 100 => .., _ => .. }` with `n: i64`, the runtime constraints are `n == "0"`, `n == "1 ..= 5"`, `n == "k"` and `n == "_"`. The analyzer's static builder encodes the binding arm `k if k > 100` as `n == "k"` (a string constant) and ignores the guard. Z3 cannot solve any of these as integer conditions, so concolic exploration cannot target match arms.

The core works around part of this for enums by matching raw token text alphanumerically (str-mambd). That does not help integer ranges, bindings, guards or wildcards.

This issue is only the match-arm semantics. Replacing the hand-formatted JSON with typed `protocol::SymExpr` + serde (and the escaping bugs that causes) is existing open **str-qwua7.36**; this issue builds on its shared builder and is blocked by it (the blocker slug `qwua7-36-escaping-repro` resolves to str-qwua7.36).

## Evidence

Re-verified against the audit worktree (main 16794cef + audit files):

- `shatter-rust/src/instrument.rs:413-446` `visit_expr_match_mut`: builds `{"kind":"bin_op","op":"eq","left":{"kind":"param","name":"<match expr tokens>"},"right":{"kind":"const","type":"str","value":"<pattern tokens>"}}` for every arm, including bindings and wildcards. The param node has no `path` field.
- `shatter-rust/src/analyzer.rs:1750-1771` walks match arms without looking at `arm.guard`. `analyzer.rs:2128-2137` (`build_pattern_sym_expr`, `syn::Pat::Ident`) produces `scrutinee == Const(Str(ident))` for a binding.
- `shatter-rust/CLAUDE.md:81-83` describes the core-side token-text workaround (str-mambd).
- Audit probe (finding frontend-rust-03, not re-run): for `clean(n)` (`n + 1 > 5`, then the 4-arm int match above), all 60 concolic iterations used n=0 and reached 3 of 7 returns, while the batch line reported "6 paths, 5/7 branches".

## Acceptance criteria

- [ ] One typed pattern encoder, in the shared builder module str-qwua7.36 introduces, is used by both `instrument.rs` and `analyzer.rs` (parallel-parity rule). No `Const(Str(<pattern text>))` is produced for int literals, ranges, bindings or wildcards.
- [ ] Arm predicates follow Rust's ordered semantics. With `P_j` = pattern of arm j and `G_j` = its guard (true if none), the predicate for arm i is `P_i ∧ G_i ∧ ¬(P_1 ∧ G_1) ∧ … ∧ ¬(P_{i-1} ∧ G_{i-1})`. Int literal → `eq(int)`. Range → `and(ge, le)` / `and(ge, lt)` for `..=` / `..`, including half-open `a..` and `..=b`. Binding → `true` with the binding name substituted by the scrutinee inside the guard. Wildcard → `true` (so it becomes the negation of all earlier arms). Or-patterns → disjunction.
- [ ] Unit tests on the encoder, each asserting a concrete scrutinee value is accepted by exactly the arm Rust would pick:
  - overlapping ranges `0..=10 => A, 5..=15 => B`: 7 → A only; 12 → B.
  - failed guard falls through: `k if k > 100 => A, 50..=200 => B, _ => C`: 150 → A, 60 → B, 10 → C.
  - wildcard after literals and ranges.
  - a proptest that, for generated i64 match arm lists (literals, ranges, guarded bindings, wildcard) and a generated scrutinee value, evaluates the encoded predicates and checks that exactly the arm Rust's first-match rule picks is true.
- [ ] Emitted runtime constraints are validated as the type actually on the wire: the instrumentor emits bare `SymExpr` JSON, and `shatter-rust-runtime/src/lib.rs:178-186` (`branch_hit`) wraps it into `SymConstraint::Expr`. Tests deserialize the emitted string as `protocol::SymExpr` (not `SymConstraint`) and check it contains no `unknown` node.
- [ ] E2E known-answer case in `shatter-core/tests/e2e_concolic_rust.rs`: an integer match whose arms need Z3 (no literal that constant mining could find, e.g. range `1000..=1003` and a guard `k if k * 3 == 6009`), plus a wildcard. The test asserts every arm is reached. Show it failing on main and passing on the branch with `SHATTER_EXAMPLES_DIR="$(python3 scripts/examples_checkout.py --no-update)" cargo test --test e2e_concolic_rust <name> -- --include-ignored` (plain `cargo test --test e2e_concolic_rust` skips all cases because they are `#[ignore]`d). Paste both `test result:` lines; each must show 1 run, 0 ignored.
- [ ] After the above is green, retire the core token-text workaround described in `shatter-rust/CLAUDE.md:81-83`, or record in the close note which part must stay (e.g. cross-file `rename_all` enum values) and why, and update that CLAUDE.md section.

## Suggested approach

After str-qwua7.36 lands, extend the shared `build_pattern_sym_expr` for ranges, bindings, guards, or-patterns and wildcards, and have the match visitor fold the earlier-arm negations in order. Keep the per-arm predicate construction in one function that both the analyzer and the instrumentor call.

## Out of scope

- The typed-SymExpr migration, serde escaping and invalid-JSON constraints (str-qwua7.36; audit repros posted there via note `qwua7-36-escaping-repro`).
- Enum/struct patterns beyond what the existing str-mambd workaround handles, unless needed to retire it.
- TS and Go instrumentors.

## Size

M

## References

- Finding frontend-rust-03 (audit 2026-09-22). Old draft: `drafts/shatter-code/59-rust-instrument-constraints.md` (split: the JSON half became note `qwua7-36-escaping-repro`).
- Related: str-qwua7.36 (open, typed SymExpr, blocker), str-mambd (closed, enum-domain token-text workaround).
