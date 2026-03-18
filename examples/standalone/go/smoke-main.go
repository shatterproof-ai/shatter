// Smoke test fixture: Go standalone file with explicit func main()
// Regression guard: shatter instrumentation must not add a second func main()
// to a package main file that already declares one.
//
// EXPECTED BRANCHES for SmokeClassify:
//   1. n > 0  -> "positive"
//   2. n <= 0 -> "non-positive"

package main

// SmokeClassify categorizes an integer for smoke testing.
func SmokeClassify(n int) string {
	if n > 0 {
		return "positive"
	}
	return "non-positive"
}

func main() {}
