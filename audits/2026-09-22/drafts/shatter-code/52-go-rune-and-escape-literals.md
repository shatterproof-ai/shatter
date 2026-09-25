# Go rune/char literals are typed as string constants in both SymExpr builders; analyzer strings.Trim mangles escapes and quotes

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P1 |
| labels | go,solver,concolic,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-qwua7.35, str-qwua7.37 |
| source findings | frontend-go-03 |

<!-- body -->
## Problem

`c == 'x'` on a rune param produces `{param c} == {const str "x"}` in both analyze and runtime constraints, so the arm is unreachable by the solver. The analyzer also uses `strings.Trim` instead of `strconv.Unquote`, leaving `\t` unconverted and stripping quote characters that belong to the value.

## Current code facts / evidence

- `shatter-go/protocol/analyzer.go:2345-2355` `strings.Trim(lit.Value, "`\"'")` for STRING and CHAR.
- `shatter-go/instrument/symextract.go:141-152` uses strconv.Unquote for STRING (correct) but emits CHAR as `{Kind:const, Type:"str"}`.
- Probe `func Classify(s string, c rune) int { if s=="a\tb"{return 1}; if s=="'q'"{return 2}; if c=='x'{return 3}; return 0 }`: analyze values `"a\\tb"`, `"q"`, and c == str 'x'. Default explore 60 iters: 6/7 lines, return 3 never reached; --concolic: 5/7 lines. Both report '3/3 branches'.
- Why concolic missed `"a\tb"` is unverified (runtime constraint has the correct tab).

## Acceptance criteria

- CHAR literals emit `{type:int, value:<codepoint>}` in both builders.
- Analyzer uses strconv.Unquote for STRING and CHAR.
- rapid property: litSymExpr(parse(strconv.Quote(s))).Value == s.
- e2e_concolic_go known-answer cases for escaped strings and rune/byte comparisons reach every arm.

## Suggested approach

Small targeted fix now; builder unification stays in str-qwua7.35.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: frontend-go-03 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-qwua7.35, str-qwua7.37
