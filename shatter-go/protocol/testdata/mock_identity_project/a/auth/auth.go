package auth

// GetName is the "a" package's function; it shares its base name ("auth") and
// function name ("GetName") with mockidentity/b/auth.GetName, so the two are
// distinguishable only by import path.
func GetName() string { return "a/auth" }
