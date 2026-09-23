# Note for str-qwua7.37: Step 0 premise is wrong for Go package functions; wrong data path

- Priority: P3
- Type: note
- Labels: agents,protocol
- Tracker: shatter (bd, /home/ketan/project/shatter)
- Relation: note-to-append (target str-qwua7.37)
- Source findings: protocol-parity-14
- Action: append as notes to str-qwua7.37 (no new issue)
- Readiness: drafted to the issue-readiness-check standard; fresh-reviewer precheck still required before filing (see INDEX.md)

<!-- body -->
Audit 2026-09-22 correction (evidence `audits/2026-09-22/areas/protocol-parity.md`):

- Step 0 says Go's `name="recv.Method", receiver=null` call form "cannot be
  solved" and must become bare name + receiver. That is wrong for
  package-qualified free functions: `shatter-core/data/string-ops.yaml:33-34`
  declares `{language: go, method: "strings.HasPrefix", style: free}`, and
  `shatter-core/src/solver.rs:~919` has a "Go-style: no receiver, two
  positional args" branch with tests. Live Go analyze output for
  `strings.HasPrefix(s, "go_")` is `{name: "strings.HasPrefix", args: [param s,
  const]}` and is solvable.
- The real divergence is method calls on values: `u.IsAdmin()` emits
  `{name: "u.IsAdmin", args: []}` (`shatter-go/protocol/analyzer.go:2359-2385`
  `callSymExpr`), dropping the receiver, so `collect_param_names`
  (`sym_expr.rs:138-145`) loses param `u`.
- Proposed amended canonical form: package-qualified free functions keep
  `name="pkg.Fn", receiver=null`; method calls on values emit `name=<bare>`,
  `receiver=<expr>`. Add a regression test that Go `strings.*` constraints stay
  solvable.
- Path fix: the file is `shatter-core/data/string-ops.yaml`, not
  `data/string-ops.yaml`.
