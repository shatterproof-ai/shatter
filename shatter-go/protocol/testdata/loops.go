package testdata

// SumUpTo sums integers from 0 to n (exclusive) using a canonical counted loop.
// Expected: loop_id=0, induction_var={name:"i", init=0, step=1, bound=n, bound_op="lt"}
func SumUpTo(n int) int {
	total := 0
	for i := 0; i < n; i++ {
		total += i
	}
	return total
}

// SumStep2 sums every second integer from 0 to n using a step of 2.
// Expected: loop_id=0, induction_var={name:"i", init=0, step=2, bound=n, bound_op="lt"}
func SumStep2(n int) int {
	total := 0
	for i := 0; i < n; i += 2 {
		total += i
	}
	return total
}

// SumDown counts down from n to 1 using decrement.
// Expected: loop_id=0, induction_var={name:"i", init=n, step=-1, bound=0, bound_op="gt"}
func SumDown(n int) int {
	total := 0
	for i := n; i > 0; i-- {
		total += i
	}
	return total
}

// ModifyIV has a body that assigns to the induction variable — should NOT be detected.
func ModifyIV(n int) int {
	total := 0
	for i := 0; i < n; i++ {
		i = i + 1 // modifies induction variable
		total += i
	}
	return total
}

// NoCond has no condition expression — should NOT produce a LoopInfo.
func NoCond() {
	for {
		break
	}
}

// RangeOnly uses a range loop — should NOT produce a LoopInfo (no induction var analysis).
func RangeOnly(s []int) int {
	total := 0
	for _, v := range s {
		total += v
	}
	return total
}
