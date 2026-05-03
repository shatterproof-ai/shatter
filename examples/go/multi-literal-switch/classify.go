// Package classify is the str-5jen regression fixture: a Go switch with a
// multi-literal case clause (`case 2, 3:`). Pre-fix the instrumentor only
// emitted the equality `x == 2` as the symbolic constraint for that
// clause, so the solver could never drive `x == 3` and that branch stayed
// unreachable. Post-fix the constraint is the disjunction
// `x == 2 || x == 3`, giving the solver targets for both literals.
package classify

// Classify maps an int into one of four buckets via a switch with a
// multi-literal case clause. The expected reachable returns are:
//
//	x == 1        -> "one"
//	x == 2 or 3   -> "small"   (multi-literal clause)
//	x == 7        -> "lucky"
//	default       -> "other"
func Classify(x int) string {
	switch x {
	case 1:
		return "one"
	case 2, 3:
		return "small"
	case 7:
		return "lucky"
	default:
		return "other"
	}
}
