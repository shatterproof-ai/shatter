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

// HelperFunc is not a handler.
func HelperFunc(s string) int {
	return len(s)
}
