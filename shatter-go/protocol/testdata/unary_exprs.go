package testdata

func AddressOfBranch(x int) bool {
	if &x != nil {
		return true
	}
	return false
}
