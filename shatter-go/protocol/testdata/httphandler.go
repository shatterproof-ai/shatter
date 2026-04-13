package testdata

import "net/http"

// HelloHandler is a standard net/http handler function.
func HelloHandler(w http.ResponseWriter, r *http.Request) {
	if r.Method == "POST" {
		w.WriteHeader(http.StatusCreated)
		w.Write([]byte("created")) //nolint:errcheck
		return
	}
	w.Header().Set("Content-Type", "text/plain")
	w.WriteHeader(http.StatusOK)
	w.Write([]byte("hello")) //nolint:errcheck
}

// NotAHandler has the wrong signature.
func NotAHandler(a, b int) int {
	return a + b
}

// PartialMatch has only one http param — should not match.
func PartialMatch(w http.ResponseWriter, n int) {
	_ = w
	_ = n
}

// Server is a type with an http handler method.
type Server struct {
	Name string
}

// Handle is a method handler on Server.
func (s *Server) Handle(w http.ResponseWriter, r *http.Request) {
	w.Write([]byte(s.Name)) //nolint:errcheck
}

// UnnamedParams has unnamed http handler params.
func UnnamedParams(http.ResponseWriter, *http.Request) {}
