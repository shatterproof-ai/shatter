package generators

import (
	"testing"
)

func TestWasmCacheMissingFileReturnsError(t *testing.T) {
	cache := NewWasmCache()
	defer cache.Close()

	_, _, _, err := cache.Generate("/nonexistent/plugin.wasm", "generate", nil)
	if err == nil {
		t.Fatal("expected error for missing WASM file")
	}
}

func TestWasmCacheCloseIsIdempotent(t *testing.T) {
	cache := NewWasmCache()
	cache.Close()
	cache.Close() // should not panic
}
