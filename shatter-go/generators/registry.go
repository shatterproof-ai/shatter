package generators

import (
	"encoding/json"
	"fmt"
	"path/filepath"
)

// GeneratorFunc is the signature for native (compiled-in) generators.
type GeneratorFunc func(recipe json.RawMessage) GeneratorResult

// Registry dispatches generate requests to WASM plugins or native generators.
type Registry struct {
	Wasm   *WasmCache
	Native map[string]GeneratorFunc
}

func NewRegistry() *Registry {
	return &Registry{
		Wasm:   NewWasmCache(),
		Native: make(map[string]GeneratorFunc),
	}
}

// Generate dispatches to the appropriate backend based on file extension.
// .wasm files are loaded via Extism; .go files look up a native GeneratorFunc
// by name; all others return an error.
func (r *Registry) Generate(file, name string, recipe json.RawMessage) (json.RawMessage, string, json.RawMessage, error) {
	ext := filepath.Ext(file)
	switch ext {
	case ".wasm":
		return r.Wasm.Generate(file, name, recipe)
	case ".go":
		fn, ok := r.Native[name]
		if !ok {
			return nil, "", nil, fmt.Errorf("native generator %q not registered", name)
		}
		result := fn(recipe)
		return result.Value, result.ID, result.Recipe, nil
	default:
		return nil, "", nil, fmt.Errorf("unsupported generator type %q", ext)
	}
}

// Close releases resources held by all backends.
func (r *Registry) Close() {
	r.Wasm.Close()
}
