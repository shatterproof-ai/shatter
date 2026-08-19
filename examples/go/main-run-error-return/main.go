// Package main is the str-wvfke fixture reducing the kapow observation to an
// in-repo reproduction: a `package main` entrypoint helper named `run`,
// called by `main` (which then calls `os.Exit`), whose first guard makes a
// non-nil `error` unavoidable for empty input.
//
// In the field, kapow's api/cmd/fixtureutil/main.go::run and
// api/cmd/testlanes/main.go::run both explored with 100 iterations, 0 lines
// covered, and a null canonical return value against an analyzed `error`
// return type -- and were misclassified `Class 1 -- returns void` instead of
// reporting the observed/thrown behavior. This fixture exercises the same
// shape (package main, function named `run`, non-void `error` return, called
// by `main` -> `os.Exit`) so `cargo test --test e2e_concolic_go` can assert
// against it directly, and so the (A) investigation into the underlying
// empty-`lines_executed` observability gap has a concrete, in-repo target.
//
// `run` is a plain analyzable function (unlike `main`, which the analyzer
// filters as an entrypoint -- see examples/go/main-os-exit).
package main

import (
	"fmt"
	"os"
)

func main() {
	if err := run(""); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

// run returns an error when name is empty and nil otherwise. The empty-input
// guard mirrors kapow's unavoidable-first-guard shape: every input reaching
// the "" branch produces a non-nil error, and no input produces a void
// (unobserved) return.
func run(name string) error {
	if name == "" {
		return fmt.Errorf("name is required")
	}
	return nil
}
