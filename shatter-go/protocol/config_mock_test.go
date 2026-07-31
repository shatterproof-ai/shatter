package protocol

import (
	"encoding/json"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/shatter-dev/shatter/shatter-go/config"
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

// mockOnlyConfig builds a config.File carrying a single mock for
// "importer.go:Run", for tests that inject a config loader instead of writing
// YAML to disk.
func mockOnlyConfig(symbol, expression string) config.File {
	return config.File{
		Functions: map[string]config.FunctionConfig{
			"importer.go:Run": {Mocks: map[string]string{symbol: expression}},
		},
	}
}

// TestExecute_PrepareIDRevalidatesMockFingerprint is the str-hr40t regression
// test: an execute that names a cached harness by an explicit prepare_id must
// notice that the config mocks changed since prepare and rebuild, rather than
// silently running the harness built under the old substitutions.
//
// The rebuilt harness is pre-registered under the NEW prepare_id so the test
// exercises the invalidation seam without paying for a real `go build`.
func TestExecute_PrepareIDRevalidatesMockFingerprint(t *testing.T) {
	root := t.TempDir()
	target := filepath.Join(root, "importer.go")
	if err := os.WriteFile(target, []byte("package main\n\nfunc Run() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	cfg := mockOnlyConfig("dep.NewThing", "&dep.Thing{N: 1}")
	h := NewHandlerWithLogLevel(strings.NewReader(""), io.Discard, io.Discard, "error")
	h.policyConfigLoader = func(string) (config.File, error) { return cfg, nil }

	fnName := "Run"
	oldMocks := h.resolveExecMocks(target, fnName, nil)
	oldID := computePrepareID(target, fnName, oldMocks, "")
	oldHarness := &fakePreparedExecution{
		preparedProvenance: preparedProvenance{mockFingerprint: instrument.MockFingerprint(oldMocks)},
	}
	h.preparedHarnesses[oldID] = oldHarness
	h.preparedTargets[target+"\x00"+fnName+"\x00"+""+"\x00"] = oldID

	// The operator edits .shatter/config.yaml mid-session: same symbol, new
	// substitution expression.
	cfg = mockOnlyConfig("dep.NewThing", "&dep.Thing{N: 2}")
	newMocks := h.resolveExecMocks(target, fnName, nil)
	newID := computePrepareID(target, fnName, newMocks, "")
	if newID == oldID {
		t.Fatalf("test setup: mutated config must change the derived prepare_id")
	}
	newHarness := &fakePreparedExecution{
		preparedProvenance: preparedProvenance{mockFingerprint: instrument.MockFingerprint(newMocks)},
		InvokeResult:       &instrument.ExecuteResult{ReturnValue: json.RawMessage(`2`)},
	}
	h.preparedHarnesses[newID] = newHarness

	resp := h.handleExecute(Response{ProtocolVersion: ProtocolVersion, ID: 1}, Request{
		ProtocolVersion: ProtocolVersion,
		ID:              1,
		Command:         "execute",
		File:            target,
		Function:        &fnName,
		PrepareID:       &oldID,
	})

	if resp.Status != "execute" {
		t.Fatalf("status = %q, want execute (message: %s)", resp.Status, resp.Message)
	}
	if resp.Outcome == nil || string(resp.Outcome.ReturnValue) != "2" {
		t.Fatalf("stale harness served the execute: outcome = %+v", resp.Outcome)
	}
	if !oldHarness.Cleaned {
		t.Errorf("stale harness must be cleaned up on fingerprint mismatch")
	}
	if _, still := h.preparedHarnesses[oldID]; still {
		t.Errorf("stale prepare_id %q must be evicted from the harness cache", oldID)
	}
	if id, still := h.preparedTargets[target+"\x00"+fnName+"\x00"+""+"\x00"]; still {
		t.Errorf("stale target registration must be dropped, still points at %q", id)
	}
}

// TestExecute_PrepareIDReusesHarnessWhenMocksUnchanged is the negative half of
// str-hr40t: an unchanged config must NOT invalidate the prepared harness, or
// every execute would pay a rebuild.
func TestExecute_PrepareIDReusesHarnessWhenMocksUnchanged(t *testing.T) {
	root := t.TempDir()
	target := filepath.Join(root, "importer.go")
	if err := os.WriteFile(target, []byte("package main\n\nfunc Run() {}\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	cfg := mockOnlyConfig("dep.NewThing", "&dep.Thing{N: 1}")
	h := NewHandlerWithLogLevel(strings.NewReader(""), io.Discard, io.Discard, "error")
	h.policyConfigLoader = func(string) (config.File, error) { return cfg, nil }

	fnName := "Run"
	mocks := h.resolveExecMocks(target, fnName, nil)
	prepareID := computePrepareID(target, fnName, mocks, "")
	harness := &fakePreparedExecution{
		preparedProvenance: preparedProvenance{mockFingerprint: instrument.MockFingerprint(mocks)},
		InvokeResult:       &instrument.ExecuteResult{ReturnValue: json.RawMessage(`1`)},
	}
	h.preparedHarnesses[prepareID] = harness

	resp := h.handleExecute(Response{ProtocolVersion: ProtocolVersion, ID: 1}, Request{
		ProtocolVersion: ProtocolVersion,
		ID:              1,
		Command:         "execute",
		File:            target,
		Function:        &fnName,
		PrepareID:       &prepareID,
	})

	if resp.Status != "execute" {
		t.Fatalf("status = %q, want execute (message: %s)", resp.Status, resp.Message)
	}
	if harness.Cleaned {
		t.Errorf("unchanged config must not invalidate the prepared harness")
	}
	if _, ok := h.preparedHarnesses[prepareID]; !ok {
		t.Errorf("unchanged config must leave prepare_id %q cached", prepareID)
	}
	if resp.Outcome == nil || string(resp.Outcome.ReturnValue) != "1" {
		t.Fatalf("prepared harness should have served the execute: outcome = %+v", resp.Outcome)
	}
}
