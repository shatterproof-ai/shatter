package generators

import "encoding/json"

// GeneratorResult is the expected JSON output from a WASM generator plugin.
// The plugin must return {"id": "...", "value": ..., "recipe": ...}.
type GeneratorResult struct {
	ID     string          `json:"id"`
	Value  json.RawMessage `json:"value"`
	Recipe json.RawMessage `json:"recipe,omitempty"`
}

// NativeGeneratorResult is returned by compiled-in (custom build) generators.
// Value holds a live in-process object; Recipe is a serializable blob for replay.
type NativeGeneratorResult struct {
	ID     string          // Human-readable label (required).
	Value  any             // Live object for in-process use.
	Recipe json.RawMessage // Serializable reconstruction params.
}

// NativeGeneratorFunc is the signature for custom-build generators.
type NativeGeneratorFunc func(recipe json.RawMessage) NativeGeneratorResult
