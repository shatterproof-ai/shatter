package generators

import (
	"encoding/json"
	"testing"
)

func TestRegistryDispatchesWasmExtension(t *testing.T) {
	r := NewRegistry()
	defer r.Close()

	// .wasm dispatch should attempt to load the file (and fail since it doesn't exist)
	_, _, _, err := r.Generate("nonexistent.wasm", "gen", nil)
	if err == nil {
		t.Fatal("expected error for nonexistent .wasm file")
	}
}

func TestRegistryDispatchesGoExtension(t *testing.T) {
	r := NewRegistry()
	defer r.Close()

	// .go dispatch without a registered native generator should return an error
	_, _, _, err := r.Generate("gen.go", "MyGen", nil)
	if err == nil {
		t.Fatal("expected error for unregistered native generator")
	}
	if got := err.Error(); got != `native generator "MyGen" not registered (custom build required)` {
		t.Errorf("unexpected error: %v", got)
	}
}

func TestRegistryNativeGeneratorReturnsResult(t *testing.T) {
	r := NewRegistry()
	defer r.Close()

	r.RegisterNative("User", func(recipe json.RawMessage) NativeGeneratorResult {
		return NativeGeneratorResult{
			ID:     "user-gen",
			Value:  map[string]any{"name": "Alice", "age": 30},
			Recipe: json.RawMessage(`{"name":"Alice","age":30}`),
		}
	})

	value, id, recipe, err := r.Generate("gen.go", "User", nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if id != "user-gen" {
		t.Errorf("generator_id = %q, want %q", id, "user-gen")
	}
	// Native generators return a sentinel with __shatter_native and handle ID.
	var sentinel map[string]any
	if err := json.Unmarshal(value, &sentinel); err != nil {
		t.Fatalf("failed to parse sentinel: %v", err)
	}
	if sentinel["__shatter_native"] != true {
		t.Errorf("expected __shatter_native sentinel, got %s", string(value))
	}
	if sentinel["handle"] == nil {
		t.Error("expected handle ID in sentinel")
	}
	if string(recipe) != `{"name":"Alice","age":30}` {
		t.Errorf("recipe = %s, want {\"name\":\"Alice\",\"age\":30}", string(recipe))
	}
}

func TestRegistryNativeGeneratorHandleResolution(t *testing.T) {
	r := NewRegistry()
	defer r.Close()

	type LiveObj struct{ Name string }
	r.RegisterNative("Conn", func(recipe json.RawMessage) NativeGeneratorResult {
		return NativeGeneratorResult{
			ID:     "test-conn",
			Value:  &LiveObj{Name: "test"},
			Recipe: json.RawMessage(`{"name":"test"}`),
		}
	})

	value, _, _, err := r.Generate("gen.go", "Conn", nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	var sentinel map[string]any
	if err := json.Unmarshal(value, &sentinel); err != nil {
		t.Fatalf("failed to parse sentinel: %v", err)
	}

	handleID, ok := sentinel["handle"].(string)
	if !ok {
		t.Fatalf("handle should be a string, got %T", sentinel["handle"])
	}

	obj := r.ResolveHandle(handleID)
	if obj == nil {
		t.Fatal("handle resolved to nil")
	}
	live, ok := obj.(*LiveObj)
	if !ok {
		t.Fatalf("expected *LiveObj, got %T", obj)
	}
	if live.Name != "test" {
		t.Errorf("live.Name = %q, want %q", live.Name, "test")
	}
}

func TestRegistryUnsupportedExtension(t *testing.T) {
	r := NewRegistry()
	defer r.Close()

	_, _, _, err := r.Generate("gen.py", "MyGen", nil)
	if err == nil {
		t.Fatal("expected error for unsupported extension")
	}
	if err.Error() != `unsupported generator type ".py"` {
		t.Errorf("unexpected error: %v", err)
	}
}

func TestHandleTableClear(t *testing.T) {
	ht := NewHandleTable()
	id1 := ht.Store("val1")
	id2 := ht.Store("val2")
	if ht.Len() != 2 {
		t.Errorf("expected 2 handles, got %d", ht.Len())
	}
	if ht.Resolve(id1) != "val1" {
		t.Error("id1 should resolve to val1")
	}
	if ht.Resolve(id2) != "val2" {
		t.Error("id2 should resolve to val2")
	}
	ht.Clear()
	if ht.Len() != 0 {
		t.Errorf("expected 0 handles after clear, got %d", ht.Len())
	}
	if ht.Resolve(id1) != nil {
		t.Error("id1 should resolve to nil after clear")
	}
}
