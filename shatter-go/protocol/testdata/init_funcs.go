package testdata

// Fixture for str-qo1.8: synthetic package init must be excluded from
// analyzer output so it never becomes an executable target.

var initSeed int

func init() {
	initSeed = 1
}

func init() {
	initSeed += 2
}

// PostInit is a normal target the analyzer must still surface.
func PostInit(n int) int {
	return n + initSeed
}
