package multifilesvc

// politeGreeter is the default Greeter implementation.
type politeGreeter struct {
	prefix string
}

func (g *politeGreeter) Greet(name string) string {
	if name == "" {
		return g.prefix + "stranger"
	}
	return g.prefix + name
}

// NewGreeter returns a Greeter that prepends the given prefix to every name.
// The return type Greeter is declared in iface.go; a single-file typechecker
// cannot resolve it, but the packages-based loader loads both files together.
func NewGreeter(prefix string) Greeter {
	return &politeGreeter{prefix: prefix}
}

// Classify categorises the input length: negative lengths are invalid, zero
// is empty, and positive lengths are non-empty. This function provides
// concrete branch targets for the concolic engine.
func Classify(input string) int {
	n := len(input)
	if n < 0 {
		return -1
	}
	if n == 0 {
		return 0
	}
	return 1
}
