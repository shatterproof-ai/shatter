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

func TestWasmCachePluginCaching(t *testing.T) {
	cache := NewWasmCache()
	defer cache.Close()

	// Both calls should fail (no file), but the error path exercises caching logic.
	// The first call populates the cache miss path; the second would hit cache
	// if the first had succeeded.
	_, _, _, err1 := cache.Generate("/nonexistent/plugin.wasm", "gen", nil)
	_, _, _, err2 := cache.Generate("/nonexistent/plugin.wasm", "gen", nil)

	if err1 == nil || err2 == nil {
		t.Fatal("expected errors for missing WASM file")
	}
	// Both should fail with loading errors since the file doesn't exist;
	// the important thing is no panic or unexpected behavior.
}

func TestWasmCacheCloseIsIdempotent(t *testing.T) {
	cache := NewWasmCache()
	cache.Close()
	cache.Close() // should not panic
}
