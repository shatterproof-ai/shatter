// Package main is the str-1hlk.17.3 fixture: conditional reassignment across
// an if/else followed by a second branch that references the reassigned
// variable, demonstrating that the Go analyzer emits an ite SymExpr for the
// second branch's condition (label resolves to ite(x>0, 1, -1)).
package main

// Categorize assigns label based on the sign of x, then returns a scaled
// value determined by the sign of label.  The two-branch shape ensures:
//
//   - Branch 0 (x > 0): simple parameter comparison, no ite.
//   - Branch 1 (label > 0): label resolved via flow-map to
//     ite{condition: x > 0, then_expr: 1, else_expr: -1}, so the static
//     condition becomes ite(x>0,1,-1) > 0.
//
// Concolic exploration finds two paths: x > 0 yields 2, x <= 0 yields 0.
func Categorize(x int) int {
	var label int
	if x > 0 {
		label = 1
	} else {
		label = -1
	}
	if label > 0 {
		return label * 2
	}
	return 0
}
