package generators

import (
	"context"
	"encoding/json"
	"fmt"
	"sync"

	extism "github.com/extism/go-sdk"
)

// WasmCache loads and caches Extism plugins keyed by file path.
// Plugins are created on first use and reused for subsequent calls.
type WasmCache struct {
	mu      sync.Mutex
	plugins map[string]*extism.Plugin
}

func NewWasmCache() *WasmCache {
	return &WasmCache{plugins: make(map[string]*extism.Plugin)}
}

// Generate loads the WASM plugin at wasmPath (caching it), calls funcName
// with the JSON-encoded recipe as input, and parses the plugin output as
// a GeneratorResult. Returns (value, generator_id, recipe, error).
func (c *WasmCache) Generate(wasmPath, funcName string, recipe json.RawMessage) (json.RawMessage, string, json.RawMessage, error) {
	plugin, err := c.getOrLoad(wasmPath)
	if err != nil {
		return nil, "", nil, fmt.Errorf("loading WASM plugin %q: %w", wasmPath, err)
	}

	// Use recipe as input; empty string if nil.
	var input []byte
	if recipe != nil {
		input = recipe
	}

	_, output, err := plugin.Call(funcName, input)
	if err != nil {
		return nil, "", nil, fmt.Errorf("calling %q in %q: %w", funcName, wasmPath, err)
	}

	var result GeneratorResult
	if err := json.Unmarshal(output, &result); err != nil {
		return nil, "", nil, fmt.Errorf("parsing generator output from %q: %w", wasmPath, err)
	}

	return result.Value, result.ID, result.Recipe, nil
}

func (c *WasmCache) getOrLoad(wasmPath string) (*extism.Plugin, error) {
	c.mu.Lock()
	defer c.mu.Unlock()

	if p, ok := c.plugins[wasmPath]; ok {
		return p, nil
	}

	manifest := extism.Manifest{
		Wasm: []extism.Wasm{
			extism.WasmFile{Path: wasmPath},
		},
	}

	plugin, err := extism.NewPlugin(context.Background(), manifest, extism.PluginConfig{
		EnableWasi: true,
	}, nil)
	if err != nil {
		return nil, err
	}

	c.plugins[wasmPath] = plugin
	return plugin, nil
}

// Close frees all cached plugins.
func (c *WasmCache) Close() {
	c.mu.Lock()
	defer c.mu.Unlock()

	for path, p := range c.plugins {
		p.Close(context.Background())
		delete(c.plugins, path)
	}
}
