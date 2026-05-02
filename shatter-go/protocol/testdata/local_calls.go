package testdata

// Helper is called by Caller below — an intra-package, local-function call.
// The analyzer should emit Helper as a dependency of Caller so that the
// CallGraph builder can construct an edge between them.
func Helper(x int) int {
	if x < 0 {
		return 0
	}
	return x + 1
}

// Annotate is a sibling helper — exercising multiple intra-package deps.
func Annotate(x int) int {
	return x * 2
}

// Caller invokes two local functions in the same package. These bare-identifier
// calls must be reported as dependencies for the call graph to have any edges
// in projects whose calls are mostly intra-package.
func Caller(x int) int {
	y := Helper(x)
	return Annotate(y)
}
