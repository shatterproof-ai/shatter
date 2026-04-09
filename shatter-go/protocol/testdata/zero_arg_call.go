package testdata

// isReady is a helper that takes no arguments.
func isReady() bool {
	return true
}

// CheckReady branches on a zero-argument function call.
// This exercises the SymExpr serialization path where a call
// expression has an empty args slice, which must not be omitted.
func CheckReady(msg string) string {
	if isReady() {
		return "ready: " + msg
	}
	return "not ready"
}
