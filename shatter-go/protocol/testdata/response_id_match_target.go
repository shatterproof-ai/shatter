// Fixture for str-jeen.52: a working target paired with a sibling target
// whose body references an undefined identifier so that any execute against
// the package fails at the build stage. Used by response_id_match_test.go to
// confirm a frontend-side build failure stays paired with its own request and
// does not shift the protocol stream.
package responseidmatch

// WorkingAdd is a trivially buildable free function used as the
// "successful request" neighbor to BrokenSibling.
func WorkingAdd(a, b int) int {
	return a + b
}

// BrokenSibling intentionally references an undeclared identifier
// (shatterUndeclaredSymbol) so go/packages reports a type error and any
// downstream `go build` against this package fails. Analyze still succeeds
// under the lenient loader path; the failure surfaces during instrument or
// execute, which is the regression scenario for str-jeen.52.
func BrokenSibling() int {
	return shatterUndeclaredSymbol
}
