// Package main is the str-jeen.35 mixed-package fixture.
//
// The fixture reproduces the str-x0sv scenario: a target source file declared
// `package main` that lives in a directory containing sibling `_test.go`
// files (one internal `package main`, one external `package main_test`).
// The shatter-go build pipeline renames the target package to
// `shattertarget` via overlay before invoking `go build`. Without the
// str-x0sv fix, `go build -overlay` then sees the directory as containing
// "found packages shattertarget (admissions.go) and main (..._test.go)" and
// fails. With the fix, the rewritten `_test.go` siblings are staged into the
// overlay so the loader sees a consistent `shattertarget` declaration.
//
// Regression test:
//   shatter-go/build/builder_mixedpkg_fixture_test.go
//
// Cross-link: str-x0sv (original bug + harness) and str-jeen.35 (this
// fixture under examples/go/).
package main

// Compute is the load-bearing target. The branching body gives the
// instrumented launcher pipeline real branch data to record so the test
// could be extended to assert recorder output, but the fixture's primary
// purpose is the `go build -overlay` regression on the package boundary.
func Compute(n int) string {
	if n > 0 {
		return "positive"
	}
	return "nonpositive"
}

func main() {}
