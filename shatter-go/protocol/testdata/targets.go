package testdata

// Increment is an exported free function. The analyzer should classify it as
// TargetKindFunction with no receiver and visibility "exported".
func Increment(n int) int {
	return n + 1
}

// hidden is an unexported free function. The analyzer should classify it as
// TargetKindFunction with visibility "unexported".
func hidden() bool {
	return true
}

// Counter is a simple struct used to test method receiver shapes.
type Counter struct {
	value int
}

// Add has a value receiver. The analyzer should classify it as TargetKindMethod
// with ReceiverShape{TypeName: "Counter", IsPointer: false} and qualified name
// "(Counter).Add".
func (c Counter) Add(n int) int {
	return c.value + n
}

// Reset has a pointer receiver. The analyzer should classify it as
// TargetKindMethod with ReceiverShape{TypeName: "Counter", IsPointer: true}
// and qualified name "(*Counter).Reset".
func (c *Counter) Reset() {
	c.value = 0
}
