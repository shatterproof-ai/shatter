package multifilepkg

// Validate reports whether the result satisfies the basic shape contract.
func Validate(result DiscoveryResult) bool {
	if !result.OK {
		return false
	}
	return result.Name != ""
}
