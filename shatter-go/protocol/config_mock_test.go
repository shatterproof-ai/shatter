package protocol

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/shatter-dev/shatter/shatter-go/instrument"
)

func writeConfigFixture(t *testing.T, cfg string) (root, target string) {
	t.Helper()
	root = t.TempDir()
	target = filepath.Join(root, "importer.go")
	if err := os.WriteFile(target, []byte("package main\n\nfunc Run() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	shatterDir := filepath.Join(root, ".shatter")
	if err := os.MkdirAll(shatterDir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(shatterDir, "config.yaml"), []byte(cfg), 0o644); err != nil {
		t.Fatal(err)
	}
	return root, target
}

// TestConfigMockConfigs_LoadsExpressionMocks verifies the str-c8djq execute-
// time bridge: `.shatter/config.yaml` `mocks` entries for a target are loaded
// and converted into expression-bearing instrument.MockConfig values, which is
// what makes config mocks affect execution (not just planning).
func TestConfigMockConfigs_LoadsExpressionMocks(t *testing.T) {
	_, target := writeConfigFixture(t, `functions:
  "importer.go:Run":
    mocks:
      "scraper.NewContext": "&scraper.Context{Fake: true}"
      "http.Get": "fakeResponse()"
`)
	h := newPreflightHandler()
	got := h.configMockConfigs(target, "Run")
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
	if got := newPreflightHandler().configMockConfigs(target, "Run"); got != nil {
		t.Fatalf("expected nil for missing config, got %+v", got)
	}
}

// TestConfigMockConfigs_UnmatchedFunction returns nil when the config has
// mocks but none for the requested target function.
func TestConfigMockConfigs_UnmatchedFunction(t *testing.T) {
	_, target := writeConfigFixture(t, `functions:
  "importer.go:SomethingElse":
    mocks:
      "scraper.NewContext": "&scraper.Context{}"
`)
	if got := newPreflightHandler().configMockConfigs(target, "Run"); got != nil {
		t.Fatalf("expected nil for unmatched function, got %+v", got)
	}
}

// TestConfigMockConfigs_CachesByMtime verifies the parsed config is memoized
// (str-c8djq review fix 4) and re-read only when the file mtime changes.
func TestConfigMockConfigs_CachesByMtime(t *testing.T) {
	root, target := writeConfigFixture(t, `functions:
  "importer.go:Run":
    mocks:
      "a.B": "fake()"
`)
	h := newPreflightHandler()
	if got := h.configMockConfigs(target, "Run"); len(got) != 1 {
		t.Fatalf("first load: expected 1 mock, got %+v", got)
	}
	if len(h.configCache) != 1 {
		t.Fatalf("expected config to be cached, cache=%v", h.configCache)
	}
	// Rewrite with new content and a bumped mtime; the cache must refresh.
	cfgPath := filepath.Join(root, ".shatter", "config.yaml")
	if err := os.WriteFile(cfgPath, []byte(`functions:
  "importer.go:Run":
    mocks:
      "a.B": "fake()"
      "c.D": "other()"
`), 0o644); err != nil {
		t.Fatal(err)
	}
	future := time.Now().Add(3 * time.Second)
	if err := os.Chtimes(cfgPath, future, future); err != nil {
		t.Fatal(err)
	}
	if got := h.configMockConfigs(target, "Run"); len(got) != 2 {
		t.Fatalf("after mtime bump: expected 2 mocks, got %+v", got)
	}
}

// TestConfigMockConfigs_MalformedReturnsNil ensures a malformed config yields
// no mocks (rather than crashing) and degrades gracefully; the WARN log is
// emitted as a side effect (verified by manual inspection).
func TestConfigMockConfigs_MalformedReturnsNil(t *testing.T) {
	_, target := writeConfigFixture(t, "functions: [this is not valid: mapping\n")
	if got := newPreflightHandler().configMockConfigs(target, "Run"); got != nil {
		t.Fatalf("expected nil for malformed config, got %+v", got)
	}
}

// TestDedupeMocks_ConfigWinsOverWire proves a wire mock and a config mock for
// the same symbol collapse to the expression-bearing entry (review fix 2),
// preventing a duplicate ShatterMock declaration.
func TestDedupeMocks_ConfigWinsOverWire(t *testing.T) {
	deduped := instrument.DedupeMocks([]instrument.MockConfig{
		{Symbol: "auth:GetAccount", ReturnValues: []any{nil}},
		{Symbol: "auth.GetAccount", Expression: "&auth.Account{}"},
	})
	if len(deduped) != 1 {
		t.Fatalf("expected 1 deduped mock, got %d: %+v", len(deduped), deduped)
	}
	if deduped[0].Expression != "&auth.Account{}" {
		t.Fatalf("expected config expression to win, got %+v", deduped[0])
	}
}
