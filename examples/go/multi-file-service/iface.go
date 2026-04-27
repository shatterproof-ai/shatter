// Package multifilesvc demonstrates a multi-file Go package. The Shatter Go
// frontend loads all files in the package together so that sibling type
// declarations are visible during analysis — a function whose return type is
// declared in a different file resolves correctly under the packages-based
// analyzer (str-hy9b.C2).
package multifilesvc

// Greeter is a service interface declared in this file. Functions in
// service.go that return Greeter are resolved correctly when both files are
// loaded together.
type Greeter interface {
	Greet(name string) string
}
