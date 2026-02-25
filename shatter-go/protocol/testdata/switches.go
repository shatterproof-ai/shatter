package testdata

// SwitchOnString uses a switch statement.
func SwitchOnString(color string) int {
	switch color {
	case "red":
		return 1
	case "green":
		return 2
	case "blue":
		return 3
	default:
		return 0
	}
}

// ForLoop uses a for loop with condition.
func ForLoop(n int) int {
	sum := 0
	for i := 0; i < n; i++ {
		sum += i
	}
	return sum
}

// LogicalOps uses logical operators in conditions.
func LogicalOps(a, b int) string {
	if a > 0 && b > 0 {
		return "both_positive"
	}
	if a > 0 || b > 0 {
		return "one_positive"
	}
	return "none_positive"
}
