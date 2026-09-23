---
slug: go-concolic-escaped-string-miss
kind: new
title: "Concolic Go explore misses `s == \"a\\tb\"` although the runtime constraint carries the real tab: find where the escaped value is lost and fix it"
priority: P2
type: bug
labels: [go-frontend, solver, concolic, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [go-rune-and-escape-literals]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Concolic Go explore misses `s == "a\tb"` although the runtime constraint carries the real tab

## Problem

In the audit probe below, `shatter explore --concolic` never reached `return 1`. The runtime `branch_path` constraint for `s == "a\tb"` already carries the decoded tab (`symextract.go` uses `strconv.Unquote` for STRING literals), so the analyzer's `strings.Trim` bug (fixed by `go-rune-and-escape-literals`) may not explain it. Candidate causes: the orchestrator prefers the static (mangled) constant, the core's string encoding of control characters to/from Z3 loses the tab, or the solved value is re-encoded wrongly on the way back into the execute request. None has been checked.

## Evidence

- Audit probe (finding frontend-go-03), `lit.go`:
  ```go
  func Classify(s string, c rune) int {
      if s == "a\tb" { return 1 }
      if s == "'q'"  { return 2 }
      if c == 'x'    { return 3 }
      return 0
  }
  ```
  `--concolic`, 200 iterations (stopped at 24): 5/7 lines, `return 1` and `return 3` missed. The default explorer reached `return 1` in 60 iterations (probably through literal harvesting, whose `strconv.Unquote` path is correct).
- `shatter-go/instrument/symextract.go:139-150`: STRING literals decoded with `strconv.Unquote`.
- `shatter-go/protocol/analyzer.go:2351-2353`: the static builder's `strings.Trim` (fixed by the blocking issue).

## Acceptance criteria

- [ ] After `go-rune-and-escape-literals` lands, re-run the probe with `--concolic` and record the result in the issue. If `return 1` is now reached, add the E2E case below, record the run, and close with that evidence (no engine change needed).
- [ ] Otherwise, record the root cause in the issue with a trace showing where the tab is lost (solver model value, execute request JSON, or frontend input decoding), and fix it there.
- [ ] Either way, `shatter-core/tests/e2e_concolic_go.rs` gains a known-answer concolic case for string compares against literals containing `\t`, `\n`, `\\` and `\x00`, each arm reached. It fails on the commit before the fix (or, in the no-change case, on main before `go-rune-and-escape-literals`), and passes after; record the failing output.
- [ ] If the defect is in the core's string encoding, a proptest in shatter-core: for random strings including control characters, a string constant encoded to Z3 and back through the model is byte-identical.
- [ ] `task e2e-go` passes (it runs `cargo test --test e2e_concolic_go -- --include-ignored`); paste the cargo summary line showing `0 ignored` and the new test name. `task affected` passes with `Gates selected` recorded.

## Out of scope

- Literal decoding/typing in the Go builders (`go-rune-and-escape-literals`).
- The "3/3 branches" metric while arms are missed (engine-correctness bucket).

## Dependencies

- Blocked by: `go-rune-and-escape-literals`.

## Size

S-M

## References

- Finding frontend-go-03 (audit 2026-09-22; evidence `audits/2026-09-22/areas/frontend-go.md` go-03). Split out of `go-rune-and-escape-literals` after the Codex cross-check (finding 5: its E2E AC required reaching an arm whose cause was out of scope).
