package handler

import "context"

// UseContext gives the Go planner a stable conformance target whose first
// parameter should be satisfied by the runtime-value registry.
func UseContext(ctx context.Context) string {
	if ctx == nil {
		return "missing"
	}
	return "ok"
}
