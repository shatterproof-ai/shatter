package testdata

// DeeplyNested simulates a large generated type graph (e.g. openapi3.T) where
// struct nesting exceeds the TypeInfo depth cap. Used by the depth-limit
// regression test (str-eyta).

type L1 struct {
	Name string
	L2   L2
}

type L2 struct {
	Value int
	L3    L3
}

type L3 struct {
	Flag bool
	L4   L4
}

type L4 struct {
	Label string
	L5    L5
}

type L5 struct {
	Count int
	L6    L6
}

type L6 struct {
	Tag  string
	L7   L7
}

type L7 struct {
	Score float64
	L8    L8
}

type L8 struct {
	Active bool
	L9     L9
}

type L9 struct {
	Note string
	L10  L10
}

type L10 struct {
	Extra string
}

// ProcessDeep accepts a parameter whose type nesting exceeds MaxTypeInfoDepth.
// The analyzer must complete without memory blow-up; fields below the depth cap
// must be represented as "unknown" rather than fully expanded.
func ProcessDeep(doc L1) string {
	if doc.Name == "" {
		return "empty"
	}
	return "ok"
}
