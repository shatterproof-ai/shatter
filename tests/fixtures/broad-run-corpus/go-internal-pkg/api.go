// Package api exposes a public entry point that delegates to an
// internal/ package. This shape exposes harness import-rewrite bugs
// where the generated harness fails to resolve the internal import.
package api

import "example.com/internalpkg/internal/svc"

// Process classifies the input via the internal service.
func Process(n int) string {
	return svc.Classify(n)
}
