// Package pkg provides a function that survives across both passes
// of the stale-source phase.
package pkg

// Keep returns a label for n, regardless of which sibling files exist.
func Keep(n int) string {
	if n < 0 {
		return "neg"
	}
	if n == 0 {
		return "zero"
	}
	return "pos"
}
