# rewrite-syntax fixture (str-jeen.34)

Acceptance fixture for the Go-frontend rewrite/instrumentation path.

The single source file `rewrite_syntax.go` collects Go language constructs
that have historically tripped the AST rewriter against real-world
codebases (see `docs/validation/2026-04-go-frontend-kapow-rerun.md`):

- Generic functions with multiple type parameters and type sets.
- Pointer- and value-receiver methods on the same type.
- Named return values, variadic parameters, multiple return values.
- Type switches (with and without an init clause).
- `for`/range loops over slices, maps, and channels.
- Function literals (closures) capturing outer parameters, including
  parameters reassigned after the closure is constructed (the case the
  visitor's `isReassignedAfter` guard exists to handle).
- `defer`/`go` statements with bound argument expressions.
- Receive (`<-ch`) and send (`ch <- v`) channel operations — the
  `<-` token previously caused `buildUnOp` to emit a synthetic binary
  op name (str-gq7c).
- Address-of (`&x`) and dereference (`*p`) — also covered by str-gq7c.
- Embedded struct/interface fields.
- Anonymous struct literals and short variable declarations across all
  control-flow forms.

The acceptance contract is in
`shatter-go/build/builder_rewrite_syntax_fixture_test.go`: every file
emitted by `instrument.InstrumentPackageForOverlay` against this fixture
must parse cleanly with `go/parser` and contain at least one
`__shatter_record_*` call. This guards against regressions where the
rewriter would silently produce invalid Go (the v1 "syntax errors: 56"
bucket described in the kapow validation log).
