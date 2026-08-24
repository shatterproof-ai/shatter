package build_test

import (
	"context"
	"fmt"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/shatter-dev/shatter/shatter-go/build"
	"github.com/shatter-dev/shatter/shatter-go/workspace"
	"pgregory.net/rapid"
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

func TestBinaryRegistryStaleInstancesPersistOnlyCurrentMutation(t *testing.T) {
	dir := t.TempDir()
	oldTarget := t.TempDir()
	newTarget := t.TempDir()
	disjointTarget := t.TempDir()

	seed := build.NewBinaryRegistry(dir)
	if err := seed.Register("same-hash", oldTarget); err != nil {
		t.Fatalf("seed same-hash: %v", err)
	}

	// Model two processes that loaded the same old value. The first updates
	// that value, then the stale second process registers a disjoint key. Its
	// old in-memory snapshot must not roll the first process's update back.
	r1 := build.NewBinaryRegistry(dir)
	r2 := build.NewBinaryRegistry(dir)
	if err := r1.Register("same-hash", newTarget); err != nil {
		t.Fatalf("update same-hash: %v", err)
	}
	if err := r2.Register("disjoint-hash", disjointTarget); err != nil {
		t.Fatalf("register disjoint-hash: %v", err)
	}

	reloaded := build.NewBinaryRegistry(dir)
	wants := map[string]string{
		"same-hash":     newTarget,
		"disjoint-hash": disjointTarget,
	}
	for hash, want := range wants {
		got, ok := reloaded.Lookup(hash)
		if !ok {
			t.Fatalf("persisted registry lost %s", hash)
		}
		if got != want {
			t.Errorf("Lookup(%q) = %q, want %q", hash, got, want)
		}
	}
}

func TestBinaryRegistryRegisterConcurrentWithWorkspaceGC(t *testing.T) {
	ws, err := workspace.Open(t.TempDir())
	if err != nil {
		t.Fatalf("open workspace: %v", err)
	}
	if err := ws.Ensure(); err != nil {
		t.Fatalf("ensure workspace: %v", err)
	}

	const registrations = 64
	registries := make([]*build.BinaryRegistry, registrations)
	for i := range registries {
		registries[i] = build.NewBinaryRegistry(ws.BinariesDir())
	}

	stopGC := make(chan struct{})
	gcStarted := make(chan struct{})
	gcDone := make(chan struct{})
	gcErrors := make(chan error, 1)
	go func() {
		defer close(gcDone)
		first := true
		for {
			select {
			case <-stopGC:
				return
			default:
			}
			_, runErr := ws.RunGC(workspace.GCOptions{
				KeepLastN:         -1,
				MaxAge:            time.Nanosecond,
				MaxRunsBytes:      -1,
				MaxCacheBytes:     -1,
				MaxGeneratedBytes: -1,
				MaxBinariesBytes:  -1,
				Now:               time.Now().Add(time.Hour),
			})
			if first {
				close(gcStarted)
				first = false
			}
			if runErr != nil {
				select {
				case gcErrors <- runErr:
				default:
				}
				return
			}
		}
	}()
	<-gcStarted

	start := make(chan struct{})
	registerErrors := make(chan error, registrations)
	var workers sync.WaitGroup
	for i, registry := range registries {
		workers.Add(1)
		go func(index int, registry *build.BinaryRegistry) {
			defer workers.Done()
			<-start
			if err := registry.Register(
				fmt.Sprintf("hash-%02d", index),
				fmt.Sprintf("/nonexistent/binary-%02d", index),
			); err != nil {
				registerErrors <- err
			}
		}(i, registry)
	}
	close(start)
	workers.Wait()
	close(stopGC)
	<-gcDone

	select {
	case err := <-gcErrors:
		t.Fatalf("RunGC raced with registry persistence: %v", err)
	default:
	}
	close(registerErrors)
	for err := range registerErrors {
		t.Errorf("Register raced with RunGC: %v", err)
	}

	reloaded := build.NewBinaryRegistry(ws.BinariesDir())
	if got := reloaded.Len(); got != registrations {
		t.Fatalf("persisted registry contains %d entries, want %d", got, registrations)
	}
}

func TestBinaryRegistryMutationSequenceMatchesLastWrite(t *testing.T) {
	rapid.Check(t, func(rt *rapid.T) {
		dir := t.TempDir()
		targets := []string{t.TempDir(), t.TempDir(), t.TempDir()}
		registries := []*build.BinaryRegistry{
			build.NewBinaryRegistry(dir),
			build.NewBinaryRegistry(dir),
			build.NewBinaryRegistry(dir),
		}
		want := make(map[string]string)
		operationCount := rapid.IntRange(1, 12).Draw(rt, "operation-count")
		for operation := 0; operation < operationCount; operation++ {
			registryIndex := rapid.IntRange(0, len(registries)-1).
				Draw(rt, fmt.Sprintf("registry-%d", operation))
			keyIndex := rapid.IntRange(0, 2).
				Draw(rt, fmt.Sprintf("key-%d", operation))
			targetIndex := rapid.IntRange(0, len(targets)-1).
				Draw(rt, fmt.Sprintf("target-%d", operation))
			key := fmt.Sprintf("hash-%d", keyIndex)
			if err := registries[registryIndex].Register(key, targets[targetIndex]); err != nil {
				rt.Fatalf("Register(%q): %v", key, err)
			}
			want[key] = targets[targetIndex]
		}

		reloaded := build.NewBinaryRegistry(dir)
		if got := reloaded.Len(); got != len(want) {
			rt.Fatalf("persisted registry contains %d entries, want %d", got, len(want))
		}
		for key, wantPath := range want {
			gotPath, ok := reloaded.Lookup(key)
			if !ok {
				rt.Fatalf("persisted registry lost %s", key)
			}
			if gotPath != wantPath {
				rt.Fatalf("Lookup(%q) = %q, want %q", key, gotPath, wantPath)
			}
		}
	})
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
