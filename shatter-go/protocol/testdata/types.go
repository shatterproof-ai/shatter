package testdata

// Point is a simple struct.
type Point struct {
	X float64
	Y float64
}

// Order represents an order with nested types.
type Order struct {
	ID       int
	Items    []string
	Priority string
	Total    float64
}

// Distance computes distance from origin for a point.
func Distance(p Point) float64 {
	return p.X*p.X + p.Y*p.Y
}

// ProcessOrder handles an order with branching on struct fields.
func ProcessOrder(order Order) string {
	if order.Priority == "express" {
		return "expedited"
	}
	if order.Total > 100.0 {
		return "free_shipping"
	}
	return "standard"
}

// ScaleSlice multiplies each element in a slice.
func ScaleSlice(values []float64, factor float64) []float64 {
	result := make([]float64, len(values))
	for i, v := range values {
		result[i] = v * factor
	}
	return result
}

// LookupMap retrieves a value from a map.
func LookupMap(m map[string]int, key string) (int, bool) {
	v, ok := m[key]
	return v, ok
}

// ProcessPointer dereferences a pointer.
func ProcessPointer(p *Point) float64 {
	if p == nil {
		return 0
	}
	return p.X + p.Y
}
