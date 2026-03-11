package generators

import (
	"encoding/json"
	"fmt"
	"path/filepath"
)

// Registry dispatches generate requests to WASM plugins or native generators.
type Registry struct {
	Wasm    *WasmCache
	Native  map[string]NativeGeneratorFunc
	Handles *HandleTable
}

func NewRegistry() *Registry {
	return &Registry{
		Wasm:    NewWasmCache(),
		Native:  make(map[string]NativeGeneratorFunc),
		Handles: NewHandleTable(),
	}
}

// RegisterNative adds a compiled-in generator function by name.
func (r *Registry) RegisterNative(name string, fn NativeGeneratorFunc) {
	r.Native[name] = fn
}

// Generate dispatches to the appropriate backend based on file extension.
// .wasm files are loaded via Extism; .go files look up a native NativeGeneratorFunc
// by name; all others return an error.
// Returns (value, generator_id, recipe, error).
func (r *Registry) Generate(file, name string, recipe json.RawMessage) (json.RawMessage, string, json.RawMessage, error) {
	ext := filepath.Ext(file)
	switch ext {
	case ".wasm":
		return r.Wasm.Generate(file, name, recipe)
	case ".go":
		fn, ok := r.Native[name]
		if !ok {
			return nil, "", nil, fmt.Errorf("native generator %q not registered (custom build required)", name)
		}
		result := fn(recipe)
		// Store live object in handle table; return sentinel to core.
		handleID := r.Handles.Store(result.Value)
		sentinel, _ := json.Marshal(map[string]any{
			"__shatter_native": true,
			"handle":           handleID,
		})
		return sentinel, result.ID, result.Recipe, nil
	default:
		return nil, "", nil, fmt.Errorf("unsupported generator type %q", ext)
	}
}

// ResolveHandle returns the live object for a native generator handle.
func (r *Registry) ResolveHandle(handleID string) any {
	return r.Handles.Resolve(handleID)
}

// Close releases resources held by all backends.
func (r *Registry) Close() {
	r.Wasm.Close()
	r.Handles.Clear()
}
