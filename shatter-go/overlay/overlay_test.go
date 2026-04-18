package overlay

import (
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

func writeFile(t *testing.T, path, content string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatalf("mkdir %s: %v", filepath.Dir(path), err)
	}
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatalf("write %s: %v", path, err)
	}
}

func TestBuilder_AddWritesManifest(t *testing.T) {
	workspace := t.TempDir()
	realA := filepath.Join(workspace, "real_a.go")
	realB := filepath.Join(workspace, "gen/real_b.go")
	writeFile(t, realA, "package x\n")
	writeFile(t, realB, "package y\n")

	overlaysDir := filepath.Join(workspace, "overlays")
	b := NewBuilder(overlaysDir, "plan-123")

	inTreeA := filepath.Join(workspace, "mod/pkg/a.go")
	if err := b.Add(inTreeA, realA); err != nil {
		t.Fatalf("Add: %v", err)
	}
	anchor := filepath.Join(workspace, "mod/pkg")
	if err := b.AddGenerated(realB, anchor, "shatter_generated/b.go"); err != nil {
		t.Fatalf("AddGenerated: %v", err)
	}

	manifestPath, err := b.Write()
	if err != nil {
		t.Fatalf("Write: %v", err)
	}
	if !filepath.IsAbs(manifestPath) {
		t.Fatalf("manifestPath not absolute: %q", manifestPath)
	}
	if filepath.Base(manifestPath) != "plan-123.json" {
		t.Fatalf("unexpected manifest filename: %q", manifestPath)
	}

	raw, err := os.ReadFile(manifestPath)
	if err != nil {
		t.Fatalf("read manifest: %v", err)
	}
	var got Manifest
	if err := json.Unmarshal(raw, &got); err != nil {
		t.Fatalf("unmarshal manifest: %v", err)
	}
	wantInTreeA, _ := filepath.Abs(inTreeA)
	wantRealA, _ := filepath.Abs(realA)
	wantInTreeB, _ := filepath.Abs(filepath.Join(anchor, "shatter_generated/b.go"))
	wantRealB, _ := filepath.Abs(realB)

	if got.Replace[wantInTreeA] != wantRealA {
		t.Errorf("mapping for A: got %q, want %q", got.Replace[wantInTreeA], wantRealA)
	}
	if got.Replace[wantInTreeB] != wantRealB {
		t.Errorf("mapping for B: got %q, want %q", got.Replace[wantInTreeB], wantRealB)
	}
	if len(got.Replace) != 2 {
		t.Errorf("Replace length: got %d, want 2", len(got.Replace))
	}
}

func TestBuilder_WriteFailsOnMissingRealFile(t *testing.T) {
	workspace := t.TempDir()
	overlaysDir := filepath.Join(workspace, "overlays")
	b := NewBuilder(overlaysDir, "plan-1")

	if err := b.Add(filepath.Join(workspace, "mod/a.go"), filepath.Join(workspace, "does_not_exist.go")); err != nil {
		t.Fatalf("Add: %v", err)
	}

	_, err := b.Write()
	if err == nil {
		t.Fatalf("expected error, got nil")
	}
	if !strings.Contains(err.Error(), "stat") {
		t.Errorf("error does not mention stat: %v", err)
	}

	if _, err := os.Stat(filepath.Join(overlaysDir, "plan-1.json")); !os.IsNotExist(err) {
		t.Errorf("manifest should not exist; stat err: %v", err)
	}
}

func TestBuilder_WriteFailsOnDirectoryRealPath(t *testing.T) {
	workspace := t.TempDir()
	realDir := filepath.Join(workspace, "a_directory")
	if err := os.MkdirAll(realDir, 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	overlaysDir := filepath.Join(workspace, "overlays")
	b := NewBuilder(overlaysDir, "plan-1")
	if err := b.Add(filepath.Join(workspace, "mod/a.go"), realDir); err != nil {
		t.Fatalf("Add: %v", err)
	}
	_, err := b.Write()
	if err == nil {
		t.Fatalf("expected error, got nil")
	}
	if !strings.Contains(err.Error(), "not a regular file") {
		t.Errorf("error does not mention regular file: %v", err)
	}
}

func TestBuilder_CollisionRejected(t *testing.T) {
	workspace := t.TempDir()
	realA := filepath.Join(workspace, "a.go")
	realB := filepath.Join(workspace, "b.go")
	writeFile(t, realA, "package x\n")
	writeFile(t, realB, "package x\n")

	b := NewBuilder(filepath.Join(workspace, "overlays"), "plan")
	inTree := filepath.Join(workspace, "mod/a.go")

	if err := b.Add(inTree, realA); err != nil {
		t.Fatalf("first Add: %v", err)
	}
	if err := b.Add(inTree, realA); err != nil {
		t.Fatalf("duplicate identical Add should be no-op: %v", err)
	}
	if err := b.Add(inTree, realB); err == nil {
		t.Fatalf("conflicting Add should fail")
	}
}

func TestBuilder_EmptyInputsRejected(t *testing.T) {
	b := NewBuilder(t.TempDir(), "plan")
	if err := b.Add("", "/tmp/x"); err == nil {
		t.Error("empty inTreePath should fail")
	}
	if err := b.Add("/tmp/y", ""); err == nil {
		t.Error("empty realPath should fail")
	}
	if err := b.AddGenerated("", "anchor", "b.go"); err == nil {
		t.Error("empty realFile should fail")
	}
	if err := b.AddGenerated("/tmp/x", "", "b.go"); err == nil {
		t.Error("empty anchor should fail")
	}
	if err := b.AddGenerated("/tmp/x", "anchor", ""); err == nil {
		t.Error("empty basename should fail")
	}
}

func TestBuilder_WriteRequiresNonEmptyDirAndPlanID(t *testing.T) {
	b := NewBuilder("", "plan")
	if _, err := b.Write(); err == nil {
		t.Error("empty overlaysDir should fail")
	}
	b = NewBuilder(t.TempDir(), "")
	if _, err := b.Write(); err == nil {
		t.Error("empty planID should fail")
	}
}

// TestBuilder_GoBuildOverlaySmokeTest is the spec §D2 acceptance criterion:
// a manifest with one wrapper + one launcher builds successfully under
// `go build -overlay <manifest>` against a trivial fixture.
func TestBuilder_GoBuildOverlaySmokeTest(t *testing.T) {
	goBin, err := exec.LookPath("go")
	if err != nil {
		t.Skip("go not on PATH")
	}

	fixture := t.TempDir()
	// Trivial target module: launcher package + library package with a stub.
	writeFile(t, filepath.Join(fixture, "go.mod"), "module example.com/fixture\n\ngo 1.23\n")
	writeFile(t, filepath.Join(fixture, "lib/orig.go"), `package lib

func Value() int { return 0 }
`)
	writeFile(t, filepath.Join(fixture, "launcher/main.go"), `package main

import (
	"fmt"
	"example.com/fixture/lib"
)

func main() { fmt.Println(lib.Value()) }
`)

	// Generated files sitting outside the module tree.
	workspace := t.TempDir()
	realWrapper := filepath.Join(workspace, "generated/wrapper.go")
	writeFile(t, realWrapper, `package lib

func Wrapped() int { return Value() + 1 }
`)
	realLauncher := filepath.Join(workspace, "generated/launcher_main.go")
	writeFile(t, realLauncher, `package main

import (
	"fmt"
	"example.com/fixture/lib"
)

func main() { fmt.Println(lib.Wrapped()) }
`)

	b := NewBuilder(filepath.Join(workspace, "overlays"), "smoke-plan")
	if err := b.AddGenerated(realWrapper, filepath.Join(fixture, "lib"), "shatter_wrapper.go"); err != nil {
		t.Fatalf("AddGenerated wrapper: %v", err)
	}
	if err := b.Add(filepath.Join(fixture, "launcher/main.go"), realLauncher); err != nil {
		t.Fatalf("Add launcher: %v", err)
	}

	manifestPath, err := b.Write()
	if err != nil {
		t.Fatalf("Write: %v", err)
	}

	outBin := filepath.Join(workspace, "out")
	cmd := exec.Command(goBin, "build", "-overlay", manifestPath, "-o", outBin, "./launcher")
	cmd.Dir = fixture
	cmd.Env = append(os.Environ(), "GOFLAGS=")
	output, err := cmd.CombinedOutput()
	if err != nil {
		t.Fatalf("go build -overlay failed: %v\noutput:\n%s", err, output)
	}
	if info, err := os.Stat(outBin); err != nil || info.Size() == 0 {
		t.Fatalf("binary missing or empty at %s: err=%v", outBin, err)
	}
}
