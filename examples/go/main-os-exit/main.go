// Package main is the str-jeen.55 fixture for a CLI entrypoint whose body
// terminates the process via os.Exit. Pre-fix, scan would dispatch this
// `main` through the launcher subprocess, the os.Exit call would terminate
// the harness before any response was written, and the Go session reader
// surfaced "launcher: subprocess exited unexpectedly" — Zolem broad-run
// scans then misclassified that as a launcher infrastructure failure. The
// fix filters `func main()` at the analyzer so the launcher never runs it.
// `Compute` is a non-entrypoint helper that must remain discoverable.
package main

import "os"

func main() {
	os.Exit(2)
}

// Compute returns 1 for positive x, -1 otherwise. Lives in the same CLI
// package as `main`; analyzer must still surface it after filtering main.
func Compute(x int) int {
	if x > 0 {
		return 1
	}
	return -1
}
