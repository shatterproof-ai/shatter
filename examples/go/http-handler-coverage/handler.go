// Package httphandlercoverage is the str-1qd5i E2E fixture: a net/http
// HandlerFunc whose executed lines are gated on the request method, so GET and
// POST drive distinct line coverage. Before str-1qd5i the adapter launcher path
// ran the handler through httptest but returned empty instrumentation fields by
// construction, so the concolic pipeline saw no branch_path / lines_executed.
// This fixture proves the launcher now threads real coverage end-to-end.
//
// Known-answer method branches (via MethodBranchHandler):
//
//	GET  -> 200, body "listed"
//	POST -> 201, body "created"
package httphandlercoverage

import "net/http"

// MethodBranchHandler is a net/http.HandlerFunc that branches on the request
// method. The two arms touch disjoint source lines, so the instrumented
// launcher must report non-empty, method-dependent lines_executed.
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
