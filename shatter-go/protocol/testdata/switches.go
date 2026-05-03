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

// MultiLiteralSwitch is the str-5jen regression fixture: a switch whose
// middle clause carries two literals (`case 2, 3:`). Pre-fix the analyzer
// emitted only `x == 2` as the symbolic condition for that clause and the
// solver could never target `x == 3`. Post-fix the symbolic condition is
// the disjunction `x == 2 || x == 3`.
func MultiLiteralSwitch(x int) string {
	switch x {
	case 1:
		return "one"
	case 2, 3:
		return "small"
	default:
		return "other"
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
