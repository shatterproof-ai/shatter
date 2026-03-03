package generators

import "encoding/json"

// GeneratorResult is the expected JSON output from a WASM generator plugin.
// The plugin must return {"id": "...", "value": ..., "recipe": ...}.
type GeneratorResult struct {
	ID     string          `json:"id"`
	Value  json.RawMessage `json:"value"`
	Recipe json.RawMessage `json:"recipe,omitempty"`
}
