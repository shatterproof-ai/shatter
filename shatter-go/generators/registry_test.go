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
	if err.Error() != `native generator "MyGen" not registered` {
		t.Errorf("unexpected error: %v", err)
	}
}

func TestRegistryNativeGeneratorReturnsResult(t *testing.T) {
	r := NewRegistry()
	defer r.Close()

	r.Native["User"] = func(recipe json.RawMessage) GeneratorResult {
		return GeneratorResult{
			ID:    "user-gen",
			Value: json.RawMessage(`{"name":"Alice","age":30}`),
		}
	}

	value, id, recipe, err := r.Generate("gen.go", "User", nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if id != "user-gen" {
		t.Errorf("generator_id = %q, want %q", id, "user-gen")
	}
	if string(value) != `{"name":"Alice","age":30}` {
		t.Errorf("value = %s, want {\"name\":\"Alice\",\"age\":30}", string(value))
	}
	if recipe != nil {
		t.Errorf("recipe should be nil, got %s", string(recipe))
	}
}

func TestRegistryNativeGeneratorWithRecipe(t *testing.T) {
	r := NewRegistry()
	defer r.Close()

	r.Native["Counter"] = func(recipe json.RawMessage) GeneratorResult {
		var n int
		if recipe != nil {
			json.Unmarshal(recipe, &n) //nolint:errcheck
		}
		n++
		recipeOut, _ := json.Marshal(n)
		valOut, _ := json.Marshal(n * 10)
		return GeneratorResult{
			ID:     "counter-gen",
			Value:  valOut,
			Recipe: recipeOut,
		}
	}

	value, id, recipe, err := r.Generate("gen.go", "Counter", nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if id != "counter-gen" {
		t.Errorf("generator_id = %q, want %q", id, "counter-gen")
	}
	if string(value) != "10" {
		t.Errorf("value = %s, want 10", string(value))
	}

	// Feed the recipe back
	value2, _, _, err := r.Generate("gen.go", "Counter", recipe)
	if err != nil {
		t.Fatalf("unexpected error on second call: %v", err)
	}
	if string(value2) != "20" {
		t.Errorf("value = %s, want 20", string(value2))
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
