package main

import "net/http"

// MethodBranchHandler is a net/http handler whose executed lines are gated on
// the request method, so GET and POST drive distinct line coverage. Used by
// the adapter-launcher instrumentation tests (str-1qd5i) to prove the adapter
// path now reports non-empty, method-dependent lines_executed.
func MethodBranchHandler(w http.ResponseWriter, r *http.Request) {
	if r.Method == http.MethodPost {
		w.Header().Set("Content-Type", "text/plain")
		w.WriteHeader(http.StatusCreated)
		_, _ = w.Write([]byte("created"))
		return
	}
	w.Header().Set("Content-Type", "text/plain")
	w.WriteHeader(http.StatusOK)
	_, _ = w.Write([]byte("listed"))
}

func main() {}
