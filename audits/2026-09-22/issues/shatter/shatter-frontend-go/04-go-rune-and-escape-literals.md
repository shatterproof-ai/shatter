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

## Evidence

Re-verified against the audit worktree at commit 56c86168:

- `shatter-go/protocol/analyzer.go:2351-2353` (static builder `litSymExpr` path): `case token.STRING, token.CHAR: val := strings.Trim(lit.Value, "`\"'")` then `Type: "str"`.
- The same `strings.Trim` pattern also appears as an Unquote-failure fallback at `analyzer.go:770`, `:2689`, `:2701`, `:2745` (literal harvesting); those are fallbacks after `strconv.Unquote` and are lower risk, but should be checked in the same change.
- `shatter-go/instrument/symextract.go:139-150` (runtime builder): STRING uses `strconv.Unquote` correctly, but `case token.CHAR:` at :145 also returns `&symExpr{Kind: "const", Type: "str", Value: s}`.
- `shatter-core/tests/e2e_concolic_go.rs` has no rune or string-escape known-answer case (`grep -n "rune\|'x'" ` finds none).
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
- Unverified: why concolic also missed `"a\tb"`; the runtime constraint carries the correct tab, so the miss may be in the core's string encoding. Treat as a separate question (see Out of scope).

## Acceptance criteria

- [ ] CHAR literals emit `{kind: const, type: "int", value: <codepoint>}` in both the analyzer builder and the instrument builder.
- [ ] The analyzer uses `strconv.Unquote` for STRING and CHAR literals (falling back to `unknown`, not to a trimmed raw string, on error).
- [ ] rapid property in shatter-go: for a random printable-or-escaped string `s`, the analyzer's literal SymExpr for the parsed literal `strconv.Quote(s)` has `Value == s`; and for a random rune `r`, the literal SymExpr for `strconv.QuoteRune(r)` is `{type:int, value:int(r)}` in both builders.
- [ ] `shatter-core/tests/e2e_concolic_go.rs` gains known-answer cases (fixture under `examples/go/`) for an escaped-string compare, a quote-containing string compare, and rune and byte compares; each expected arm is reached. The new cases fail on current main and pass on the branch; record the failing output in the close note.
- [ ] `cargo test --test e2e_concolic_go` passes; `task affected` passes with `Gates selected` recorded.

## Suggested approach

Small targeted fix in the two literal switch arms plus the tests. Do not wait for builder unification; that stays in str-qwua7.35 (see the `qwua7-35-four-go-builders` note). If the `"a\tb"` concolic miss persists after the analyzer fix, file it against the core string encoding with the probe above.

## Out of scope

- Unifying the four Go SymExpr builders (str-qwua7.35) and the SymExpr construction spec (str-qwua7.37).
- The branch metric reporting "3/3 branches" while arms are missed (engine-correctness bucket).
- Root-causing the concolic `"a\tb"` miss if it survives this fix.

## Dependencies

- Blocked by: none.
- Related: str-qwua7.35 (builder unification), str-qwua7.37 (SymExpr spec).

## Size

S

## References

- Finding frontend-go-03 (audit 2026-09-22, `audits/2026-09-22/findings.json`; evidence `audits/2026-09-22/areas/frontend-go.md` go-03). Old draft: `drafts/shatter-code/52-go-rune-and-escape-literals.md`.
