package mod7

// Branch returns one of three labels based on `n`.
func Branch(n int) string {
	if n < 0 {
		return "neg"
	}
	if n == 0 {
		return "zero"
	}
	return "pos"
}

// Double returns 2*n.
func Double(n int) int { return 2 * n }
