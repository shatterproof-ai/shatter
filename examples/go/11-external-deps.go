// Example 11: External dependencies with third-party packages
// Tests shatter's handling of functions that import real third-party modules.
// Unlike 06-external-deps.go which only uses stdlib, this file imports
// packages that require `go mod tidy` / `go mod download` to resolve.
//
// Uses:
//   - github.com/go-chi/chi/v5 — lightweight HTTP router
//   - github.com/rs/zerolog     — structured JSON logger

package examples

import (
	"fmt"
	"net/http"

	"github.com/go-chi/chi/v5"
	"github.com/rs/zerolog"
)

// BuildRouter — 4 branches: method is GET→"get", POST→"post", DELETE→"delete", default→"unsupported".
// Analyzer should detect the switch arms and the chi.NewRouter() external call.
//
// EXPECTED BRANCHES (4):
//   1. method == "GET"    → "get:/path"
//   2. method == "POST"   → "post:/path"
//   3. method == "DELETE" → "delete:/path"
//   4. default            → "unsupported"
func BuildRouter(method string, path string) string {
	r := chi.NewRouter()

	switch method {
	case "GET":
		r.Get(path, func(w http.ResponseWriter, r *http.Request) {})
		return fmt.Sprintf("get:%s", path)
	case "POST":
		r.Post(path, func(w http.ResponseWriter, r *http.Request) {})
		return fmt.Sprintf("post:%s", path)
	case "DELETE":
		r.Delete(path, func(w http.ResponseWriter, r *http.Request) {})
		return fmt.Sprintf("delete:%s", path)
	default:
		return "unsupported"
	}
}

// ClassifyLogLevel — 5 branches based on zerolog level constants.
// Exercises external enum/constant usage from a third-party structured logging library.
//
// EXPECTED BRANCHES (5):
//   1. level == zerolog.DebugLevel → "debug"
//   2. level == zerolog.InfoLevel  → "info"
//   3. level == zerolog.WarnLevel  → "warn"
//   4. level == zerolog.ErrorLevel → "error"
//   5. default                     → "unknown"
func ClassifyLogLevel(level zerolog.Level) string {
	switch level {
	case zerolog.DebugLevel:
		return "debug"
	case zerolog.InfoLevel:
		return "info"
	case zerolog.WarnLevel:
		return "warn"
	case zerolog.ErrorLevel:
		return "error"
	default:
		return "unknown"
	}
}
