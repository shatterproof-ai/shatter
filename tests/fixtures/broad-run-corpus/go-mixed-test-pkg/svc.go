// Package svc has both internal and external _test.go siblings. This
// shape exposes harness build bugs where the package selection picks
// up _test files unexpectedly or fails to ignore the external test
// package.
package svc

// Categorize returns a label.
func Categorize(n int) string {
	if n < 0 {
		return "neg"
	}
	return "non-neg"
}
