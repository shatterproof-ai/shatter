package wrapper_test

import (
	"bytes"
	"encoding/json"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/wrapper"
)

// threeTargets and twoCtors are the acceptance-criteria inputs.
var threeTargets = []wrapper.WrapperTarget{
	{
		ID:         "example.com/targets:Add",
		SymbolName: "Add",
		Kind:       wrapper.TargetKindFunction,
		Parameters: []wrapper.WrapperParam{
			{Name: "a", GoType: "int"},
			{Name: "b", GoType: "int"},
		},
		HasResult:    true,
		ResultGoType: "int",
	},
	{
		ID:            "example.com/targets:(*Counter).Inc",
		SymbolName:    "Inc",
		Kind:          wrapper.TargetKindMethod,
		ReceiverType:  "Counter",
		IsPointerRecv: true,
		HasResult:     false,
	},
	{
		ID:            "example.com/targets:(Counter).Get",
		SymbolName:    "Get",
		Kind:          wrapper.TargetKindMethod,
		ReceiverType:  "Counter",
		IsPointerRecv: false,
		HasResult:     true,
		ResultGoType:  "int",
	},
}

var twoCtors = []wrapper.ConstructorCandidate{
	{FuncName: "NewCounter", TargetType: "Counter"},
	{FuncName: "MustNewCounter", TargetType: "Counter"},
}

func TestGenerateWrapperIsDeterministic(t *testing.T) {
	first := wrapper.GenerateWrapper("targets", threeTargets, twoCtors)
	second := wrapper.GenerateWrapper("targets", threeTargets, twoCtors)

	if first != second {
		t.Errorf("GenerateWrapper produced different output across two calls")
	}
	if !strings.Contains(first, "func ShatterInvoke") {
		t.Error("generated code missing ShatterInvoke")
	}
	if !strings.Contains(first, "type PlanDescriptor") {
		t.Error("generated code missing PlanDescriptor")
	}
}

func TestGenerateWrapperContainsAllTargetIDs(t *testing.T) {
	src := wrapper.GenerateWrapper("targets", threeTargets, twoCtors)
	for _, t2 := range threeTargets {
		if !strings.Contains(src, t2.ID) {
			t.Errorf("generated code missing target ID %q", t2.ID)
		}
	}
}

func TestGenerateWrapperContainsConstructorCases(t *testing.T) {
	src := wrapper.GenerateWrapper("targets", threeTargets, twoCtors)
	for _, c := range twoCtors {
		want := wrapper.WrapperKindConstructorPrefix + c.FuncName
		if !strings.Contains(src, want) {
			t.Errorf("generated code missing constructor case %q", want)
		}
	}
}

func TestDiscoveryHashIsStable(t *testing.T) {
	h1 := wrapper.DiscoveryHash(threeTargets, twoCtors)
	h2 := wrapper.DiscoveryHash(threeTargets, twoCtors)
	if h1 != h2 {
		t.Errorf("hash not stable: %q != %q", h1, h2)
	}
	if len(h1) != 16 {
		t.Errorf("hash length = %d, want 16", len(h1))
	}
}

func TestDiscoveryHashChangesWithNewTarget(t *testing.T) {
	base := threeTargets[:2]
	extended := threeTargets

	h1 := wrapper.DiscoveryHash(base, twoCtors)
	h2 := wrapper.DiscoveryHash(extended, twoCtors)

	if h1 == h2 {
		t.Errorf("hash should change when a target is added, got %q both times", h1)
	}
}

func TestWriteWrapperFileSkipsRebuild(t *testing.T) {
	dir := t.TempDir()
	targets := threeTargets[:1]

	path1, fresh1, err := wrapper.WriteWrapperFile(dir, "targets", targets, nil)
	if err != nil {
		t.Fatalf("first write: %v", err)
	}
	if !fresh1 {
		t.Error("first call should be fresh")
	}

	// Second call with identical inputs — must skip the write.
	path2, fresh2, err := wrapper.WriteWrapperFile(dir, "targets", targets, nil)
	if err != nil {
		t.Fatalf("second write: %v", err)
	}
	if fresh2 {
		t.Error("second call should be stale (no rebuild)")
	}
	if path1 != path2 {
		t.Errorf("path changed: %q != %q", path1, path2)
	}
}

func TestGeneratedWrapperCompiles(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go binary not found")
	}

	modDir := t.TempDir()
	wrapperDir := t.TempDir()

	// Fixture source with three targets and two constructors.
	const targetSrc = `package targets

type Counter struct{ n int }

func NewCounter() *Counter     { return &Counter{} }
func MustNewCounter() *Counter { return &Counter{n: 1} }
func Add(a, b int) int          { return a + b }
func (c *Counter) Inc()          { c.n++ }
func (c Counter) Get() int       { return c.n }
`
	if err := os.WriteFile(filepath.Join(modDir, "targets.go"), []byte(targetSrc), 0o644); err != nil {
		t.Fatalf("write targets.go: %v", err)
	}
	if err := os.WriteFile(filepath.Join(modDir, "go.mod"), []byte("module example.com/targets\n\ngo 1.23.0\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	// Write the generated wrapper.
	wrapperPath, _, err := wrapper.WriteWrapperFile(wrapperDir, "targets", threeTargets, twoCtors)
	if err != nil {
		t.Fatalf("WriteWrapperFile: %v", err)
	}

	// Build overlay manifest.
	hash := wrapper.DiscoveryHash(threeTargets, twoCtors)
	inTreePath := filepath.Join(modDir, wrapper.WrapperFilename(hash))
	manifest := map[string]map[string]string{
		"Replace": {inTreePath: wrapperPath},
	}
	manifestJSON, err := json.MarshalIndent(manifest, "", "  ")
	if err != nil {
		t.Fatalf("marshal overlay: %v", err)
	}
	manifestPath := filepath.Join(wrapperDir, "overlay.json")
	if err := os.WriteFile(manifestPath, manifestJSON, 0o644); err != nil {
		t.Fatalf("write overlay: %v", err)
	}

	// Verify the wrapper compiles as part of the target package.
	cmd := exec.Command("go", "build", "-overlay", manifestPath, "./...")
	cmd.Dir = modDir
	cmd.Env = append(os.Environ(), "GOFLAGS=")
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		src, _ := os.ReadFile(wrapperPath)
		t.Fatalf("go build failed: %v\nstderr: %s\ngenerated wrapper:\n%s",
			err, stderr.String(), src)
	}
}

// TestGeneratedWrapperCompilesMutliReturn verifies that functions returning
// multiple values (e.g. (int, error)) produce compilable wrapper code.
// Regression for the "assignment mismatch: 1 variable but F returns 2 values" build error.
func TestGeneratedWrapperCompilesMultiReturn(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go binary not found")
	}

	modDir := t.TempDir()
	wrapperDir := t.TempDir()

	const targetSrc = `package divide

import "fmt"

func SafeDivide(a, b int) (int, error) {
	if b == 0 {
		return 0, fmt.Errorf("divide by zero")
	}
	return a / b, nil
}
`
	if err := os.WriteFile(filepath.Join(modDir, "divide.go"), []byte(targetSrc), 0o644); err != nil {
		t.Fatalf("write divide.go: %v", err)
	}
	if err := os.WriteFile(filepath.Join(modDir, "go.mod"), []byte("module example.com/divide\n\ngo 1.23.0\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	targets := []wrapper.WrapperTarget{
		{
			ID:           "example.com/divide:SafeDivide",
			SymbolName:   "SafeDivide",
			Kind:         wrapper.TargetKindFunction,
			Parameters:   []wrapper.WrapperParam{{Name: "a", GoType: "int"}, {Name: "b", GoType: "int"}},
			HasResult:    true,
			ResultGoType: "int",
			ResultCount:  2,
		},
	}

	wrapperPath, _, err := wrapper.WriteWrapperFile(wrapperDir, "divide", targets, nil)
	if err != nil {
		t.Fatalf("WriteWrapperFile: %v", err)
	}

	hash := wrapper.DiscoveryHash(targets, nil)
	inTreePath := filepath.Join(modDir, wrapper.WrapperFilename(hash))
	manifest := map[string]map[string]string{"Replace": {inTreePath: wrapperPath}}
	manifestJSON, err := json.MarshalIndent(manifest, "", "  ")
	if err != nil {
		t.Fatalf("marshal overlay: %v", err)
	}
	manifestPath := filepath.Join(wrapperDir, "overlay.json")
	if err := os.WriteFile(manifestPath, manifestJSON, 0o644); err != nil {
		t.Fatalf("write overlay: %v", err)
	}

	cmd := exec.Command("go", "build", "-overlay", manifestPath, "./...")
	cmd.Dir = modDir
	cmd.Env = append(os.Environ(), "GOFLAGS=")
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		src, _ := os.ReadFile(wrapperPath)
		t.Fatalf("go build failed: %v\nstderr: %s\ngenerated wrapper:\n%s", err, stderr.String(), src)
	}
}

func TestGeneratedWrapperContentByteIdentical(t *testing.T) {
	dir := t.TempDir()

	path1, _, err := wrapper.WriteWrapperFile(dir, "targets", threeTargets, twoCtors)
	if err != nil {
		t.Fatalf("first write: %v", err)
	}

	// Read the content from the file.
	content1, err := os.ReadFile(path1)
	if err != nil {
		t.Fatalf("read: %v", err)
	}

	// Generate again in memory and compare.
	inMemory := wrapper.GenerateWrapper("targets", threeTargets, twoCtors)

	if !bytes.Equal(content1, []byte(inMemory)) {
		t.Error("file content differs from in-memory GenerateWrapper output")
	}
}
