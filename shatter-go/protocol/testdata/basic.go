package testdata

// Add adds two integers.
func Add(a, b int) int {
	return a + b
}

// Greet returns a greeting string.
func Greet(name string) string {
	if name == "" {
		return "hello, world"
	}
	return "hello, " + name
}

// Classify classifies a number as negative, zero, or positive.
func Classify(n int) string {
	if n < 0 {
		return "negative"
	} else if n == 0 {
		return "zero"
	}
	return "positive"
}

// Max returns the larger of two floats.
func Max(a, b float64) float64 {
	if a > b {
		return a
	}
	return b
}

// IsEven returns whether n is even.
func IsEven(n int) bool {
	return n%2 == 0
}

// noExport is unexported.
func noExport() {}
