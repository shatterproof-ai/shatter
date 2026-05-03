package broadrunchurn

// ClassifyMagnitude is present at phase 1 (snapshot) and remains in phase 2.
func ClassifyMagnitude(value int) string {
	if value < 0 {
		return "negative"
	}
	if value == 0 {
		return "zero"
	}
	if value < 10 {
		return "small"
	}
	if value < 100 {
		return "medium"
	}
	return "large"
}
