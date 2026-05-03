// Package svc is one of many sibling packages used to stress
// scan denominator semantics: a large source set where, if failed
// spans are not counted toward the denominator, completed_functions
// dominates and the reported denominator is misleadingly tiny.
package svc

// Classify returns a category for the input integer.
func Classify(n int) string {
	if n < 0 {
		return "negative"
	}
	if n == 0 {
		return "zero"
	}
	if n%2 == 0 {
		return "even-positive"
	}
	return "odd-positive"
}

// Sum adds two integers.
func Sum(a, b int) int { return a + b }

// Max returns the larger of two integers.
func Max(a, b int) int {
	if a > b {
		return a
	}
	return b
}
