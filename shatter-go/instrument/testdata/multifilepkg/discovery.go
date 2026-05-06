package multifilepkg

// DiscoveryResult is a stand-in for a domain type in a multi-file package.
type DiscoveryResult struct {
	Name string
	OK   bool
}

// Discover returns a result based on a simple branch so the instrumentor
// has something to record.
func Discover(name string) DiscoveryResult {
	if name == "" {
		return DiscoveryResult{OK: false}
	}
	return DiscoveryResult{Name: name, OK: true}
}
