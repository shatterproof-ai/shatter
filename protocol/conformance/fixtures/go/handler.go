package handler

import "net/http"

// Serve is a minimal net/http handler used by conformance tests to probe the
// Go frontend's adapter_http_nethttp capability. Matching signature triggers
// invocation_model.adapter_id == "go/http-handler" during analyze.
func Serve(w http.ResponseWriter, r *http.Request) {
	w.WriteHeader(http.StatusOK)
}
