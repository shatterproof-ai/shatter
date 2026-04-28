package protocol

import (
	"bytes"
	"errors"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestAnalyzeFile_BuildTagExcluded_ReturnsTypedError verifies that a Go file
// gated by a //go:build constraint that does not match the analyzer's default
// build context surfaces as a typed *BuildTagExcludedError instead of a
// generic "target file not found in loaded package syntax" parse error. This
// is what lets the handler convert it to ErrNotSupported and lets the Rust
// core's batch_analyze soft-skip path consume it.
func TestAnalyzeFile_BuildTagExcluded_ReturnsTypedError(t *testing.T) {
	moduleRoot := t.TempDir()
	if err := os.WriteFile(filepath.Join(moduleRoot, "go.mod"),
		[]byte("module example.com/buildtag\n\ngo 1.23.0\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}
	// Sibling file present in the default build context so the package loads.
	defaultSource := `package buildtag

func Default() int { return 1 }
`
	if err := os.WriteFile(filepath.Join(moduleRoot, "default.go"),
		[]byte(defaultSource), 0o644); err != nil {
		t.Fatalf("write default.go: %v", err)
	}
	// Target file gated by a build tag the analyzer never sets.
	gatedSource := `//go:build ui

package buildtag

func GatedFunc() int { return 42 }
`
	gatedFile := filepath.Join(moduleRoot, "gated.go")
	if err := os.WriteFile(gatedFile, []byte(gatedSource), 0o644); err != nil {
		t.Fatalf("write gated.go: %v", err)
	}

	_, err := AnalyzeFile(gatedFile, "")
	if err == nil {
		t.Fatalf("AnalyzeFile on build-tag-gated file: expected error, got nil")
	}
	var btErr *BuildTagExcludedError
	if !errors.As(err, &btErr) {
		t.Fatalf("AnalyzeFile error type = %T (%v), want *BuildTagExcludedError", err, err)
	}
	if !strings.Contains(btErr.Error(), "build-tag-excluded") {
		t.Errorf("error message %q does not contain 'build-tag-excluded'", btErr.Error())
	}
	if btErr.Constraint == "" {
		t.Errorf("BuildTagExcludedError.Constraint is empty; want the parsed constraint expression")
	}
}

// TestHandleAnalyze_BuildTagExcluded_ReturnsNotSupported verifies the handler
// maps a build-tag exclusion to ErrNotSupported, which is the wire-level
// signal the Rust core's batch_analyze treats as a soft-skip rather than an
// abort.
func TestHandleAnalyze_BuildTagExcluded_ReturnsNotSupported(t *testing.T) {
	moduleRoot := t.TempDir()
	if err := os.WriteFile(filepath.Join(moduleRoot, "go.mod"),
		[]byte("module example.com/buildtag2\n\ngo 1.23.0\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}
	if err := os.WriteFile(filepath.Join(moduleRoot, "default.go"),
		[]byte("package buildtag2\n\nfunc Default() int { return 1 }\n"), 0o644); err != nil {
		t.Fatalf("write default.go: %v", err)
	}
	gatedSource := `//go:build ui

package buildtag2

func GatedFunc() int { return 42 }
`
	gatedFile := filepath.Join(moduleRoot, "gated.go")
	if err := os.WriteFile(gatedFile, []byte(gatedSource), 0o644); err != nil {
		t.Fatalf("write gated.go: %v", err)
	}

	var output bytes.Buffer
	h := NewHandler(strings.NewReader(""), &output, io.Discard)
	resp := h.handleAnalyze(Response{}, Request{Command: "analyze", File: gatedFile})
	if resp.Status != "error" {
		t.Fatalf("response status = %q, want \"error\"; resp = %+v", resp.Status, resp)
	}
	if resp.Code != ErrNotSupported {
		t.Fatalf("response code = %q, want %q (so batch_analyze soft-skips); resp = %+v",
			resp.Code, ErrNotSupported, resp)
	}
	if !strings.Contains(resp.Message, "build-tag-excluded") {
		t.Errorf("response message %q does not contain 'build-tag-excluded'", resp.Message)
	}
}
