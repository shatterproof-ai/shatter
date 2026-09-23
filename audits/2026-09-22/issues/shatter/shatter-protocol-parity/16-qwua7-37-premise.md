---
slug: qwua7-37-premise
kind: note-to-existing
title: "Note for str-qwua7.37: the Step 0 premise is wrong for Go package functions, and the data path is wrong"
priority: P3
type: note
labels: [agents, protocol, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-qwua7.37
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note for str-qwua7.37: the Step 0 premise is wrong for Go package functions, and the data path is wrong

Target: str-qwua7.37 (OPEN, P2: "Write a one-page SymExpr construction spec and record or fix the Go call-shape divergence"). Action: add a comment (`bd comments add`, or append to notes). Do not change status or priority.

## Comment text

Audit 2026-09-22 correction (finding protocol-parity-14; evidence in `audits/2026-09-22/areas/protocol-parity.md`; re-checked 2026-09-23 at 56c86168):

- Step 0 says Go's `name="recv.Method", receiver=null` call form "cannot be solved" and must become bare name plus receiver. That is wrong for package-qualified free functions:
  - `shatter-core/data/string-ops.yaml:34` declares `{ language: go, method: "strings.HasPrefix", style: free }`.
  - `shatter-core/src/solver.rs:919` has a "Go-style: no receiver, two positional args (e.g. strings.Contains(s, substr))" branch, with tests around :2420.
  - Live Go analyze output for `strings.HasPrefix(s, "go_")` is `{name: "strings.HasPrefix", args: [param s, const]}`, which is solvable today.
- The real divergence is method calls on values. `u.IsAdmin()` emits `{name: "u.IsAdmin", args: []}`, because `callSymExpr` (`shatter-go/protocol/analyzer.go:2359`) uses the whole selector as the name and drops the receiver. `collect_param_names` (`shatter-core/src/sym_expr.rs:125`) walks only receiver and args, so it loses param `u`.
- Proposed amended canonical form: package-qualified free functions keep `name="pkg.Fn", receiver=null` (matching string-ops.yaml `style: free`). Method calls on values emit `name=<bare method>, receiver=<expr>`. To tell them apart, the Go side must resolve whether the selector's X is a package identifier (`types.PkgName`) or a value.
- Add a regression test that Go `strings.*` constraints stay solvable after the change, and a test that a value-method call keeps its receiver param in `collect_param_names`.
- Path fix: the file is `shatter-core/data/string-ops.yaml`, not `data/string-ops.yaml`.
- The verifier rated this P3. The issue's acceptance text ("receiver = expr or null") already leaves room for the right answer, but the Step 0 wording would steer an implementer toward breaking free functions, so amend the body.
