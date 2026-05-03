package svc

// Classify is the internal classifier used by the public api package.
func Classify(n int) string {
	if n < 0 {
		return "neg"
	}
	if n == 0 {
		return "zero"
	}
	return "pos"
}
