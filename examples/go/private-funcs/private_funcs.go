// Package privatefuncs is a Shatter fixture for the str-z06h opt-in
// unexported-function discovery policy. It deliberately mixes an exported
// entrypoint with two unexported helpers so a scan run can demonstrate
// the default omit-private behavior and the `--all` opt-in.
package privatefuncs

// ClassifyAmount is the exported entrypoint. It dispatches to two
// unexported helpers; both are reachable via package-local wrappers when
// `shatter scan --all` is passed.
func ClassifyAmount(amount int) string {
	if amount < 0 {
		return "invalid"
	}
	if isSmall(amount) {
		return bucket(amount)
	}
	return bucket(amount)
}

// isSmall is an unexported predicate. The default `shatter scan` run
// omits it; `shatter scan --all` discovers and explores it.
func isSmall(n int) bool {
	return n <= 10
}

// bucket is an unexported helper with multiple branches so its inclusion
// produces visible coverage in the report when `--all` is set.
func bucket(n int) string {
	switch {
	case n == 0:
		return "zero"
	case n < 10:
		return "small"
	case n < 100:
		return "medium"
	case n < 1000:
		return "large"
	default:
		return "huge"
	}
}
