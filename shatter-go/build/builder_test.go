package build_test

import (
	"context"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/build"
	"github.com/shatter-dev/shatter/shatter-go/workspace"
)

func mustTempWorkspace(t *testing.T) *workspace.Workspace {
	t.Helper()
	ws, err := workspace.Open(t.TempDir())
	if err != nil {
		t.Fatalf("open workspace: %v", err)
	}
	return ws
}

// ---- Diagnostic parser tests ----

func TestParseBuildOutputEmpty(t *testing.T) {
	diags := build.ParseBuildOutput("")
	if len(diags) != 0 {
		t.Errorf("empty input: got %d diagnostics, want 0", len(diags))
	}
}

func TestParseBuildOutputSkipsPackageHeader(t *testing.T) {
	input := "# example.com/targets\n./main.go:5:3: undefined: foo\n"
	diags := build.ParseBuildOutput(input)
	if len(diags) != 1 {
		t.Fatalf("got %d diagnostics, want 1", len(diags))
	}
	if diags[0].File != "./main.go" {
		t.Errorf("File = %q, want %q", diags[0].File, "./main.go")
	}
	if diags[0].Line != 5 {
		t.Errorf("Line = %d, want 5", diags[0].Line)
	}
	if diags[0].Column != 3 {
		t.Errorf("Column = %d, want 3", diags[0].Column)
	}
	if !strings.Contains(diags[0].Message, "undefined") {
		t.Errorf("Message %q missing 'undefined'", diags[0].Message)
	}
}

func TestParseBuildOutputLineOnly(t *testing.T) {
	input := "./file.go:10: some error"
	diags := build.ParseBuildOutput(input)
	if len(diags) != 1 {
		t.Fatalf("got %d diagnostics, want 1", len(diags))
	}
	if diags[0].Line != 10 {
		t.Errorf("Line = %d, want 10", diags[0].Line)
	}
	if diags[0].Column != 0 {
		t.Errorf("Column = %d, want 0 (absent)", diags[0].Column)
	}
}

func TestParseBuildOutputUnstructuredLine(t *testing.T) {
	input := "build failed: exit status 1"
	diags := build.ParseBuildOutput(input)
	if len(diags) != 1 {
		t.Fatalf("got %d diagnostics, want 1", len(diags))
	}
	if diags[0].Kind != build.DiagnosticKindError {
		t.Errorf("Kind = %q, want %q", diags[0].Kind, build.DiagnosticKindError)
	}
	if diags[0].File != "" {
		t.Errorf("unstructured line should have empty File, got %q", diags[0].File)
	}
}

func TestParseBuildOutputMultipleErrors(t *testing.T) {
	input := "# example.com/pkg\n./a.go:1:1: error A\n./b.go:2:2: error B\n"
	diags := build.ParseBuildOutput(input)
	if len(diags) != 2 {
		t.Fatalf("got %d diagnostics, want 2", len(diags))
	}
}

func TestDiagnosticString(t *testing.T) {
	d := build.Diagnostic{Kind: build.DiagnosticKindError, File: "main.go", Line: 7, Message: "oops"}
	s := d.String()
	if !strings.Contains(s, "main.go") || !strings.Contains(s, "7") || !strings.Contains(s, "oops") {
		t.Errorf("String() = %q, missing expected fields", s)
	}
}

// ---- BinaryRegistry tests ----

func TestBinaryRegistryLookupMiss(t *testing.T) {
	r := build.NewBinaryRegistry("")
	_, ok := r.Lookup("nonexistent")
	if ok {
		t.Error("expected Lookup miss for nonexistent hash")
	}
}

func TestBinaryRegistryRegisterAndLookup(t *testing.T) {
	r := build.NewBinaryRegistry("")
	// Register a non-existent path; Lookup will evict it.
	_ = r.Register("h1", "/does/not/exist")
	_, ok := r.Lookup("h1")
	if ok {
		t.Error("expected Lookup miss for non-existent binary path")
	}
}

func TestBinaryRegistryRegisterExistingPath(t *testing.T) {
	// Use a path that actually exists (e.g., a temp directory).
	existingDir := t.TempDir()
	r := build.NewBinaryRegistry("")
	_ = r.Register("h2", existingDir)
	path, ok := r.Lookup("h2")
	if !ok {
		t.Fatal("expected Lookup hit for existing directory")
	}
	if path != existingDir {
		t.Errorf("path = %q, want %q", path, existingDir)
	}
}

func TestBinaryRegistryLen(t *testing.T) {
	r := build.NewBinaryRegistry("")
	if r.Len() != 0 {
		t.Errorf("initial Len = %d, want 0", r.Len())
	}
	_ = r.Register("a", "/x")
	if r.Len() != 1 {
		t.Errorf("after Register Len = %d, want 1", r.Len())
	}
}

func TestBinaryRegistryPersistence(t *testing.T) {
	dir := t.TempDir()
	r1 := build.NewBinaryRegistry(dir)
	target := t.TempDir() // exists
	_ = r1.Register("hash-persist", target)

	// A second registry loading from the same dir should see the entry.
	r2 := build.NewBinaryRegistry(dir)
	path, ok := r2.Lookup("hash-persist")
	if !ok {
		t.Fatal("expected persisted entry to be visible in second registry")
	}
	if path != target {
		t.Errorf("path = %q, want %q", path, target)
	}
}

// ---- BuildRequest validation tests ----

func TestBuildRequestValidation(t *testing.T) {
	cases := []struct {
		name string
		req  build.BuildRequest
		want string
	}{
		{
			name: "empty targets",
			req:  build.BuildRequest{PackageName: "p", TargetModulePath: "x", TargetModuleDir: "/x", TargetImportPath: "x", TargetPackageDir: "/x/p"},
			want: "Targets",
		},
	}
	ws := mustTempWorkspace(t)
	b := build.NewBuilder(ws)
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			_, err := b.Build(context.Background(), tc.req)
			if err == nil {
				t.Fatal("expected error, got nil")
			}
			if !strings.Contains(err.Error(), tc.want) {
				t.Errorf("error %q missing %q", err.Error(), tc.want)
			}
		})
	}
}
