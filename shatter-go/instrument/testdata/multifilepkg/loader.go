package multifilepkg

// Load resolves a key against an in-memory source.
func Load(key string, source map[string]string) (string, bool) {
	if source == nil {
		return "", false
	}
	value, ok := source[key]
	return value, ok
}
