package testdata

import (
	"fmt"
	"net/http"
)

// HandleRequest is a standard net/http handler function.
func HandleRequest(w http.ResponseWriter, r *http.Request) {
	if r.Method == "GET" {
		fmt.Fprintf(w, "Hello, World!")
	}
}

// MyServer is a type that implements the http.Handler interface.
type MyServer struct {
	Name string
}

// ServeHTTP implements http.Handler for MyServer.
func (s *MyServer) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	fmt.Fprintf(w, "Server: %s", s.Name)
}

// WriteResponse only has a ResponseWriter — partial match.
func WriteResponse(w http.ResponseWriter) {
	fmt.Fprintf(w, "partial")
}

// handlePrivate is a private exact net/http handler — should get adapter
// recognition just like an exported handler (str-n10u).
func handlePrivate(w http.ResponseWriter, r *http.Request) {
	fmt.Fprintf(w, "private handler: %s", r.URL.Path)
}

// writePartial is a private partial helper — only ResponseWriter, not an
// exact handler signature (str-n10u). Should be skipped with a clear reason.
func writePartial(w http.ResponseWriter) {
	fmt.Fprintf(w, "partial private")
}

// HelperFunc is not a handler.
func HelperFunc(s string) int {
	return len(s)
}

// init references private functions to prevent the linter from removing them.
func init() {
	_ = handlePrivate
	_ = writePartial
}
