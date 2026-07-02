package protocol

import (
	"os"
	"path/filepath"
	"testing"
)

// TestConfigMockConfigs_LoadsExpressionMocks verifies the str-c8djq execute-
// time bridge: `.shatter/config.yaml` `mocks` entries for a target are loaded
// and converted into expression-bearing instrument.MockConfig values, which is
// what makes config mocks affect execution (not just planning).
func TestConfigMockConfigs_LoadsExpressionMocks(t *testing.T) {
	root := t.TempDir()
	target := filepath.Join(root, "importer.go")
	if err := os.WriteFile(target, []byte("package main\n\nfunc Run() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	shatterDir := filepath.Join(root, ".shatter")
	if err := os.MkdirAll(shatterDir, 0o755); err != nil {
		t.Fatal(err)
	}
	cfg := `functions:
  "importer.go:Run":
    mocks:
      "scraper.NewContext": "&scraper.Context{Fake: true}"
      "http.Get": "fakeResponse()"
`
	if err := os.WriteFile(filepath.Join(shatterDir, "config.yaml"), []byte(cfg), 0o644); err != nil {
		t.Fatal(err)
	}

	got := configMockConfigs(target, "Run")
	if len(got) != 2 {
		t.Fatalf("expected 2 config mocks, got %d: %+v", len(got), got)
	}
	// Sorted by symbol: "http.Get" < "scraper.NewContext".
	if got[0].Symbol != "http.Get" || got[0].Expression != "fakeResponse()" {
		t.Errorf("mock[0] = %+v", got[0])
	}
	if got[1].Symbol != "scraper.NewContext" || got[1].Expression != "&scraper.Context{Fake: true}" {
		t.Errorf("mock[1] = %+v", got[1])
	}
	// Wire fields must be empty — these are expression substitutions.
	for _, m := range got {
		if len(m.ReturnValues) != 0 {
			t.Errorf("config mock %q should have no return values", m.Symbol)
		}
	}
}

// TestConfigMockConfigs_NoConfig returns nil cleanly when no config exists.
func TestConfigMockConfigs_NoConfig(t *testing.T) {
	root := t.TempDir()
	target := filepath.Join(root, "x.go")
	if err := os.WriteFile(target, []byte("package main\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	if got := configMockConfigs(target, "Run"); got != nil {
		t.Fatalf("expected nil for missing config, got %+v", got)
	}
}

// TestConfigMockConfigs_UnmatchedFunction returns nil when the config has
// mocks but none for the requested target function.
func TestConfigMockConfigs_UnmatchedFunction(t *testing.T) {
	root := t.TempDir()
	target := filepath.Join(root, "importer.go")
	if err := os.WriteFile(target, []byte("package main\n\nfunc Run() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	shatterDir := filepath.Join(root, ".shatter")
	if err := os.MkdirAll(shatterDir, 0o755); err != nil {
		t.Fatal(err)
	}
	cfg := `functions:
  "importer.go:SomethingElse":
    mocks:
      "scraper.NewContext": "&scraper.Context{}"
`
	if err := os.WriteFile(filepath.Join(shatterDir, "config.yaml"), []byte(cfg), 0o644); err != nil {
		t.Fatal(err)
	}
	if got := configMockConfigs(target, "Run"); got != nil {
		t.Fatalf("expected nil for unmatched function, got %+v", got)
	}
}
