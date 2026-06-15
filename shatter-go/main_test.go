package main

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/protocol"
)

// TestHintConfigResolver_AbsoluteSourceFileMatchesFilenameGlob is the str-rd0a
// wiring regression. The hint-config resolver must normalize an ABSOLUTE
// SourceFile — the form scans pass — before MatchTarget, so filename-scoped
// `defaults`/`generators` globs resolve (matching the policy resolver). Without
// the normalization the absolute path matches no filename glob and per-function
// hints silently vanish. This guards the exact main.go call site against
// regressing back to the raw SourceFile.
func TestHintConfigResolver_AbsoluteSourceFileMatchesFilenameGlob(t *testing.T) {
	dir := t.TempDir()
	shatterDir := filepath.Join(dir, ".shatter")
	if err := os.MkdirAll(shatterDir, 0o755); err != nil {
		t.Fatalf("mkdir .shatter: %v", err)
	}
	cfg := `
functions:
  "loader.go:loadOne":
    defaults:
      dir: "/fixtures/sample"
`
	if err := os.WriteFile(filepath.Join(shatterDir, "config.yaml"), []byte(cfg), 0o644); err != nil {
		t.Fatalf("write config: %v", err)
	}
	absSource := filepath.Join(dir, "internal", "fixture", "loader.go")
	if err := os.MkdirAll(filepath.Dir(absSource), 0o755); err != nil {
		t.Fatalf("mkdir src: %v", err)
	}
	if err := os.WriteFile(absSource, []byte("package fixture"), 0o644); err != nil {
		t.Fatalf("write src: %v", err)
	}

	lookup := func(string) *protocol.TargetContext {
		return &protocol.TargetContext{
			Analysis: &protocol.FunctionAnalysis{SourceFile: absSource, Name: "loadOne"},
		}
	}
	hints := hintConfigResolver(lookup)("loader.go::loadOne")
	if len(hints.Defaults) != 1 {
		t.Fatalf("expected 1 default resolved for absolute SourceFile, got %d (%+v)", len(hints.Defaults), hints.Defaults)
	}
	if _, ok := hints.Defaults["dir"]; !ok {
		t.Fatalf("expected a 'dir' default, got %+v", hints.Defaults)
	}
}
