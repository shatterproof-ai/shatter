// Package main is the str-jeen.55 fixture for the simplest CLI-shape Go
// target: a `package main` with `func main()` and a regular helper. The
// analyzer must omit `main` from the discovered-target set so scan never
// dispatches it through the launcher subprocess (which would otherwise
// surface a misleading "launcher: subprocess exited unexpectedly" error
// when `main` does anything that exits the process). The `Helper` function
// must remain discoverable so non-entrypoint targets in CLI packages stay
// explorable.
package main

import "fmt"

func main() {
	fmt.Println(Helper(7))
}

// Helper returns n+1. Two reachable shapes via simple branching keep this
// useful as a discoverability fixture without relying on the entrypoint.
func Helper(n int) int {
	if n < 0 {
		return -1
	}
	return n + 1
}
