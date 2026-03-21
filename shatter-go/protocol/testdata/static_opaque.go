package testdata

// InternalConn has no exported fields and no New*/Create* factory function.
// All fields are unexported, so it cannot be meaningfully constructed or used
// from outside the package without a factory — NoConstructor.
// Expected: param c → opaque with static_opacity "no_constructor"
type InternalConn struct {
	fd int
}

// UseInternalConn calls a function that takes an all-unexported-field struct.
func UseInternalConn(c InternalConn) int {
	return c.fd
}
