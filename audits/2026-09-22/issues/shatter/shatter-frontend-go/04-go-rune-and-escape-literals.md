---
slug: go-rune-and-escape-literals
kind: new
title: "Go rune/char literals are typed as string constants in both SymExpr builders; analyzer strings.Trim mangles escapes and quotes"
priority: P1
type: bug
labels: [go-frontend, solver, concolic, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Go rune/char literals are typed as string constants in both SymExpr builders; analyzer strings.Trim mangles escapes and quotes

## Problem

In Go a rune literal (`'x'`) is an untyped rune constant (int32). Both Go SymExpr builders emit it as `{kind: const, type: "str"}`, so a comparison `c == 'x'` on a `rune`/`byte` parameter becomes `{param c} == {const str "x"}`: ill-typed for the solver and unreachable in both explorer modes.

Separately, the static analyzer converts STRING and CHAR literals with `strings.Trim(lit.Value, "`\"'")` instead of `strconv.Unquote`. That leaves escapes unconverted (`"a\tb"` becomes the four characters `a\tb`) and strips quote characters that belong to the value (`"'q'"` becomes `q`).

This issue fixes literal decoding and typing. The concolic engine also missed the escaped-string arm although the runtime constraint was already correct; that is a separate, undiagnosed defect tracked in `go-concolic-escaped-string-miss`, and its E2E reach is not part of this issue's acceptance.

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-go/protocol/analyzer.go:2351-2353` (static builder `litSymExpr` path): `case token.STRING, token.CHAR: val := strings.Trim(lit.Value, "`\"'")` then `Type: "str"`.
- The same `strings.Trim` pattern also appears as an Unquote-failure fallback at `analyzer.go:770`, `:2689`, `:2701`, `:2745` (literal harvesting); those are fallbacks after `strconv.Unquote` and are lower risk, but should be checked in the same change.
- `shatter-go/instrument/symextract.go:139-150` (runtime builder): STRING uses `strconv.Unquote` correctly, but `case token.CHAR:` at :145 also returns `&symExpr{Kind: "const", Type: "str", Value: s}`.
- Parameter typing: `shatter-go/protocol/analyzer.go:1555-1561` (`typeInfoFromAST`; `basicTypeInfo` has the same rule) maps `rune`/`int32` to `TypeInfo{Kind: "int"}`, so `{type:int}` constants are well-typed against a `rune` param. `byte`/`uint8` map to `{Kind: "complex", ComplexKind: "go_byte"}`; whether the solver gives `go_byte` params an Int sort is not verified here (see the byte AC).
- `shatter-core/tests/e2e_concolic_go.rs` has no rune or string-escape known-answer case (`grep -n "rune\|'x'"` finds none). Every case in that file is `#[ignore]` and runs only with `--include-ignored` (`task e2e-go`, `Taskfile.yml:611-628`).
- Audit probe (finding frontend-go-03):
  ```go
  func Classify(s string, c rune) int {
      if s == "a\tb" { return 1 }
      if s == "'q'"  { return 2 }
      if c == 'x'    { return 3 }
      return 0
  }
  ```
  Analyze right-hand sides: `{"type":"str","value":"a\\tb"}`, `{"type":"str","value":"q"}`, `{"type":"str","value":"x"}`; runtime `branch_path` constraint for the rune compare: `{"kind":"const","type":"str","value":"x"}`. `shatter explore lit.go:Classify` default mode, 60 iterations: 6/7 lines, `return 3` never reached. `--concolic`, 200 iterations (stopped at 24): 5/7 lines, `return 1` and `return 3` missed. Both reported "3/3 branches".

## Acceptance criteria

- [ ] CHAR literals emit `{kind: const, type: "int", value: <codepoint>}` in both the analyzer builder and the instrument builder.
- [ ] The analyzer uses `strconv.Unquote` for STRING and CHAR literals (falling back to `unknown`, not to a trimmed raw string, on error). The four fallback sites listed above are either switched to `unknown` or justified in a code comment.
- [ ] rapid property in shatter-go: for a random printable-or-escaped string `s`, the analyzer's literal SymExpr for the parsed literal `strconv.Quote(s)` has `Value == s`; and for a random rune `r`, the literal SymExpr for `strconv.QuoteRune(r)` is `{type:int, value:int(r)}` in both builders.
- [ ] Analyzer-level known-answer test on the probe above: the analyze response's right-hand sides are `"a\tb"` (with a real tab), `"'q'"`, and `{type:int, value:120}`. Fails on main, passes on the branch.
- [ ] `shatter-core/tests/e2e_concolic_go.rs` gains known-answer cases (fixture under `examples/go/`) for a quote-containing string compare, a rune compare and a byte compare; each expected arm is reached. The new cases fail on current main and pass on the branch; record the failing output in the close note. If the byte case fails only because `go_byte` params are not Int-sorted in the solver, fix that mapping in this issue (it is the same defect from the solver's side).
- [ ] `task e2e-go` passes (it runs `cargo test --test e2e_concolic_go -- --include-ignored`; plain `cargo test --test e2e_concolic_go` skips every case). Paste the cargo summary line showing `0 ignored` and the three new test names in the output. If the gate reports a cache hit, run the cargo command directly and paste that.
- [ ] `task affected` passes with `Gates selected` recorded.

## Suggested approach

Small targeted fix in the two literal switch arms plus the tests. Do not wait for builder unification; that stays in str-qwua7.35 (see the `qwua7-35-four-go-builders` note).

## Out of scope

- The concolic miss of the escaped-string arm (`go-concolic-escaped-string-miss`).
- Unifying the four Go SymExpr builders (str-qwua7.35) and the SymExpr construction spec (str-qwua7.37).
- The branch metric reporting "3/3 branches" while arms are missed (engine-correctness bucket).

## Dependencies

- Blocked by: none.
- Blocks: `go-concolic-escaped-string-miss`.
- Related: str-qwua7.35 (builder unification), str-qwua7.37 (SymExpr spec).

## Size

S

## References

- Finding frontend-go-03 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-03). Old draft: `drafts/shatter-code/52-go-rune-and-escape-literals.md`.
