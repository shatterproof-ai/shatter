package wrapper_test

import (
	"bytes"
	"encoding/json"
	"go/ast"
	"go/token"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/wrapper"
	"golang.org/x/tools/go/packages"
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
	{FuncName: "NewCounter", TargetType: "Counter", ReturnsPointer: true},
	{FuncName: "MustNewCounter", TargetType: "Counter", ReturnsPointer: true},
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

func TestDiscoveryHashChangesWithTargetImports(t *testing.T) {
	base := []wrapper.WrapperTarget{
		{
			ID:         "example.com/targets:UseContext",
			SymbolName: "UseContext",
			Kind:       wrapper.TargetKindFunction,
			Parameters: []wrapper.WrapperParam{{Name: "ctx", GoType: "context.Context"}},
			Imports:    nil,
		},
	}
	withImport := append([]wrapper.WrapperTarget{}, base...)
	withImport[0].Imports = []string{"context"}

	h1 := wrapper.DiscoveryHash(base, nil)
	h2 := wrapper.DiscoveryHash(withImport, nil)

	if h1 == h2 {
		t.Errorf("hash should change when target imports change, got %q both times", h1)
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
	cmd := exec.Command("go", "build", "-buildvcs=false", "-overlay", manifestPath, "./...")
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

	cmd := exec.Command("go", "build", "-buildvcs=false", "-overlay", manifestPath, "./...")
	cmd.Dir = modDir
	cmd.Env = append(os.Environ(), "GOFLAGS=")
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		src, _ := os.ReadFile(wrapperPath)
		t.Fatalf("go build failed: %v\nstderr: %s\ngenerated wrapper:\n%s", err, stderr.String(), src)
	}
}

func TestGeneratedWrapperCompilesGenericInstantiation(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go binary not found")
	}

	modDir := t.TempDir()
	wrapperDir := t.TempDir()

	const targetSrc = `package ident

func Identity[T any](v T) T {
	return v
}
`
	if err := os.WriteFile(filepath.Join(modDir, "identity.go"), []byte(targetSrc), 0o644); err != nil {
		t.Fatalf("write identity.go: %v", err)
	}
	if err := os.WriteFile(filepath.Join(modDir, "go.mod"), []byte("module example.com/ident\n\ngo 1.23.0\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	targets := []wrapper.WrapperTarget{
		{
			ID:         "example.com/ident:Identity",
			SymbolName: "Identity",
			Kind:       wrapper.TargetKindFunction,
			Parameters: []wrapper.WrapperParam{
				{Name: "v", GoType: "T"},
			},
			TypeParams:   []wrapper.TypeParamInfo{{Name: "T", Constraint: "any"}},
			HasResult:    true,
			ResultGoType: "T",
			ResultCount:  1,
		},
	}

	wrapperPath, _, err := wrapper.WriteWrapperFile(wrapperDir, "ident", targets, nil)
	if err != nil {
		t.Fatalf("WriteWrapperFile: %v", err)
	}
	src, err := os.ReadFile(wrapperPath)
	if err != nil {
		t.Fatalf("read wrapper: %v", err)
	}
	if !strings.Contains(string(src), "Identity[string](v)") {
		t.Fatalf("generated wrapper missing generic instantiation call; source:\n%s", src)
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

	cmd := exec.Command("go", "build", "-buildvcs=false", "-overlay", manifestPath, "./...")
	cmd.Dir = modDir
	cmd.Env = append(os.Environ(), "GOFLAGS=")
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		t.Fatalf("go build failed: %v\nstderr: %s\ngenerated wrapper:\n%s", err, stderr.String(), src)
	}
}

// TestGenerateWrapperEmitsTargetImports is the str-jeen.33 unit-level
// regression: when a target declares Imports, GenerateWrapper must emit one
// `import "..."` line per distinct path in the union across all targets, in
// addition to the always-present core imports (encoding/json, fmt). Without
// this, qualified parameter or result types like context.Context, *pgx.Conn,
// slog.Logger reference undefined package short names in the generated file.
func TestGenerateWrapperEmitsTargetImports(t *testing.T) {
	// Distinct paths spread across two targets, with a deliberate duplicate
	// (`context`) appearing on both — the union must dedupe it.
	targetsWithImports := []wrapper.WrapperTarget{
		{
			ID:         "example.com/multi:Handle",
			SymbolName: "Handle",
			Kind:       wrapper.TargetKindFunction,
			Parameters: []wrapper.WrapperParam{
				{Name: "ctx", GoType: "context.Context"},
				{Name: "conn", GoType: "*pgx.Conn"},
			},
			Imports:      []string{"context", "example.com/stub/pgx"},
			HasResult:    true,
			ResultGoType: "int",
			ResultCount:  1,
		},
		{
			ID:         "example.com/multi:Notify",
			SymbolName: "Notify",
			Kind:       wrapper.TargetKindFunction,
			Parameters: []wrapper.WrapperParam{
				{Name: "ctx", GoType: "context.Context"},
				{Name: "logger", GoType: "*slog.Logger"},
			},
			Imports:     []string{"context", "log/slog"},
			HasResult:   false,
			ResultCount: 0,
		},
	}

	src := wrapper.GenerateWrapper("targets", targetsWithImports, nil)

	mustContain := []string{
		`"context"`,
		`"log/slog"`,
		`"example.com/stub/pgx"`,
		`"encoding/json"`,
		`"fmt"`,
	}
	for _, want := range mustContain {
		if !strings.Contains(src, want) {
			t.Errorf("generated wrapper missing import line %s\nfull source:\n%s", want, src)
		}
	}

	// Dedup invariant: each distinct import path appears at most once in the
	// generated source. This is the proptest-shaped invariant team-lead
	// requested — for every external pkg path declared in any target's
	// Imports, exactly one `"<path>"` literal appears.
	allPaths := []string{
		"context", "log/slog", "example.com/stub/pgx",
		"encoding/json", "fmt",
	}
	for _, importPath := range allPaths {
		quoted := `"` + importPath + `"`
		if got := strings.Count(src, quoted); got != 1 {
			t.Errorf("import %s appears %d times in generated source, want 1\nsource:\n%s",
				quoted, got, src)
		}
	}
}

// TestGenerateWrapperOmitsCoreImportsFromTargetImports guards against
// double-emitting `encoding/json` or `fmt` when a target's Imports list
// happens to include them — collectExtraImports filters them.
func TestGenerateWrapperOmitsCoreImportsFromTargetImports(t *testing.T) {
	targets := []wrapper.WrapperTarget{
		{
			ID:         "example.com/dup:F",
			SymbolName: "F",
			Kind:       wrapper.TargetKindFunction,
			Parameters: []wrapper.WrapperParam{{Name: "x", GoType: "int"}},
			// Deliberately include the always-emitted core imports to verify
			// deduplication. "strings" is omitted here because the param type
			// is int — including strings for an int param would produce a
			// "imported and not used" compile error. See str-jeen.73.
			Imports:      []string{"encoding/json", "fmt", "context"},
			HasResult:    true,
			ResultGoType: "int",
			ResultCount:  1,
		},
	}

	src := wrapper.GenerateWrapper("dup", targets, nil)

	for _, core := range []string{"encoding/json", "fmt"} {
		quoted := `"` + core + `"`
		if got := strings.Count(src, quoted); got != 1 {
			t.Errorf("core import %s appears %d times, want exactly 1", quoted, got)
		}
	}
	if !strings.Contains(src, `"context"`) {
		t.Error("non-core import context missing from generated source")
	}
}

// TestGenerateWrapperEmitsStringsForIOReaderParam is the str-jeen.73
// regression: a non-generic target whose parameter type resolves to a
// runtime-value candidate that uses strings.NewReader (io.Reader, io.ReadCloser)
// must include "strings" in the generated import block. Pre-fix, "strings" was
// unconditionally excluded from collectExtraImports as a "core import" and only
// added for generic targets, leaving non-generic wrappers with a bare
// strings.NewReader("") expression but no import "strings".
func TestGenerateWrapperEmitsStringsForIOReaderParam(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go binary not found")
	}

	modDir := t.TempDir()
	wrapperDir := t.TempDir()

	const targetSrc = `package ioreader

import "io"

// ReadAll is a non-generic function whose io.Reader parameter resolves to
// strings.NewReader("") via the runtime-value registry, requiring the
// "strings" import in the generated wrapper even though there are no
// generic targets.
func ReadAll(r io.Reader) int {
	buf := make([]byte, 128)
	n, _ := r.Read(buf)
	return n
}
`
	if err := os.WriteFile(filepath.Join(modDir, "ioreader.go"), []byte(targetSrc), 0o644); err != nil {
		t.Fatalf("write ioreader.go: %v", err)
	}
	if err := os.WriteFile(filepath.Join(modDir, "go.mod"), []byte("module example.com/ioreader\n\ngo 1.23.0\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	targets := []wrapper.WrapperTarget{
		{
			ID:         "example.com/ioreader:ReadAll",
			SymbolName: "ReadAll",
			Kind:       wrapper.TargetKindFunction,
			Parameters: []wrapper.WrapperParam{
				// Runtime-value candidate for io.Reader is strings.NewReader(""),
				// which requires "strings". The generated wrapper must import it.
				{Name: "r", GoType: "io.Reader", RuntimeValueExpr: `strings.NewReader("")`},
			},
			Imports:      []string{"io", "strings"},
			HasResult:    true,
			ResultGoType: "int",
			ResultCount:  1,
		},
	}

	src := wrapper.GenerateWrapper("ioreader", targets, nil)

	// Both "io" and "strings" must appear exactly once — no duplicates and no
	// missing imports.
	for _, imp := range []string{"io", "strings"} {
		quoted := `"` + imp + `"`
		if got := strings.Count(src, quoted); got != 1 {
			t.Errorf("import %s appears %d times in generated source (want 1)\nfull source:\n%s",
				quoted, got, src)
		}
	}

	// The generated wrapper must compile against the target package.
	wrapperPath, _, err := wrapper.WriteWrapperFile(wrapperDir, "ioreader", targets, nil)
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

	cmd := exec.Command("go", "build", "-buildvcs=false", "-overlay", manifestPath, "./...")
	cmd.Dir = modDir
	cmd.Env = append(os.Environ(), "GOFLAGS=")
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		got, _ := os.ReadFile(wrapperPath)
		t.Fatalf("go build failed: %v\nstderr: %s\ngenerated wrapper:\n%s",
			err, stderr.String(), got)
	}
}

// TestGeneratedWrapperBlastRadiusIsolation is the str-qo1.14 regression:
// when a package contains a pure function plus an unrelated type whose
// constructor requires an interface argument (e.g. *SSEWriter built from
// http.ResponseWriter), the wrapper generated for the package must still
// compile so the pure function is explorable. Concretely: the wrapper must
// NOT emit `_recv := NewSSEWriter()` for a constructor that takes parameters,
// because it has no way to synthesise the interface argument.
func TestGeneratedWrapperBlastRadiusIsolation(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go binary not found")
	}

	modDir := t.TempDir()
	wrapperDir := t.TempDir()

	// Pure target plus an unrelated method whose only constructor needs
	// an http.ResponseWriter — exactly the failing fixture from the bug.
	const targetSrc = `package blastradius

import (
	"net/http"
	"strings"
)

// TokenizeWords is the pure focused target.
func TokenizeWords(text string) []string {
	return strings.Fields(text)
}

// SSEWriter is the unrelated symbol that should not poison the wrapper.
type SSEWriter struct{ w http.ResponseWriter }

// NewSSEWriter requires an http.ResponseWriter — a constructor with
// parameters. The wrapper cannot synthesise this argument and must omit
// the constructor case rather than emit uncompilable code.
func NewSSEWriter(w http.ResponseWriter) *SSEWriter { return &SSEWriter{w: w} }

// Flush is the unrelated method whose receiver type SSEWriter has no
// no-arg constructor.
func (s *SSEWriter) Flush() { /* no-op */ }
`
	if err := os.WriteFile(filepath.Join(modDir, "blastradius.go"), []byte(targetSrc), 0o644); err != nil {
		t.Fatalf("write blastradius.go: %v", err)
	}
	if err := os.WriteFile(filepath.Join(modDir, "go.mod"), []byte("module example.com/blastradius\n\ngo 1.23.0\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	targets := []wrapper.WrapperTarget{
		{
			ID:         "example.com/blastradius:TokenizeWords",
			SymbolName: "TokenizeWords",
			Kind:       wrapper.TargetKindFunction,
			Parameters: []wrapper.WrapperParam{{Name: "text", GoType: "string"}},
			HasResult:  true, ResultGoType: "[]string", ResultCount: 1,
		},
		{
			ID:            "example.com/blastradius:(*SSEWriter).Flush",
			SymbolName:    "Flush",
			Kind:          wrapper.TargetKindMethod,
			ReceiverType:  "SSEWriter",
			IsPointerRecv: true,
			HasResult:     false,
		},
	}
	// NewSSEWriter requires *http.Request via http.ResponseWriter — i.e.
	// HasParams is true. Pre-fix this leaked into the wrapper.
	ctors := []wrapper.ConstructorCandidate{
		{FuncName: "NewSSEWriter", TargetType: "SSEWriter", HasParams: true},
	}

	src := wrapper.GenerateWrapper("blastradius", targets, ctors)

	// Invariant: the wrapper must not contain a parameterless call to
	// NewSSEWriter.
	if strings.Contains(src, "NewSSEWriter()") {
		t.Errorf("generated wrapper contains uncompilable parameterless call to NewSSEWriter; source:\n%s", src)
	}
	if strings.Contains(src, "constructor:NewSSEWriter") {
		t.Errorf("generated wrapper emitted a constructor case for a constructor that takes parameters; source:\n%s", src)
	}

	// And the wrapper must compile against the package — proving the pure
	// target is reachable.
	wrapperPath, _, err := wrapper.WriteWrapperFile(wrapperDir, "blastradius", targets, ctors)
	if err != nil {
		t.Fatalf("WriteWrapperFile: %v", err)
	}
	hash := wrapper.DiscoveryHash(targets, ctors)
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
	cmd := exec.Command("go", "build", "-buildvcs=false", "-overlay", manifestPath, "./...")
	cmd.Dir = modDir
	cmd.Env = append(os.Environ(), "GOFLAGS=")
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		got, _ := os.ReadFile(wrapperPath)
		t.Fatalf("go build failed: %v\nstderr: %s\ngenerated wrapper:\n%s", err, stderr.String(), got)
	}
}

// TestGeneratedWrapperCompilesUnnamedAndBlankParams is the str-qo1.7
// regression: targets whose source signature uses unnamed parameters
// (`func F(int, string)`) or the blank identifier
// (`func (r *R) F(_ int, _ string)`) must produce a wrapper file that
// compiles. Pre-fix the wrapper emitted `var _ int`, `json.Unmarshal(_)`
// and `_recv.F(_, _)` — all rejected with "cannot use _ as value or
// type". Post-fix the wrapper-local names are stable `_p<index>`
// identifiers.
func TestGeneratedWrapperCompilesUnnamedAndBlankParams(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go binary not found")
	}

	modDir := t.TempDir()
	wrapperDir := t.TempDir()

	const targetSrc = `package blank

type Extractor struct{}

// ExtractFunction uses the blank identifier for both parameters; the
// wrapper must not reference _ on the call site.
func (e *Extractor) ExtractFunction(_ int, _ string) string { return "ok" }

// AddUnnamed declares two truly unnamed parameters.
func AddUnnamed(int, int) int { return 0 }
`
	if err := os.WriteFile(filepath.Join(modDir, "blank.go"), []byte(targetSrc), 0o644); err != nil {
		t.Fatalf("write blank.go: %v", err)
	}
	if err := os.WriteFile(filepath.Join(modDir, "go.mod"), []byte("module example.com/blank\n\ngo 1.23.0\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	// Names supplied here mirror what extractWrapperParams now produces
	// for unnamed/blank parameters. Hard-coding _p0/_p1 here also locks
	// the contract: any change to the synthetic-name shape will break
	// this test along with the internal-test invariant.
	targets := []wrapper.WrapperTarget{
		{
			ID:            "example.com/blank:(*Extractor).ExtractFunction",
			SymbolName:    "ExtractFunction",
			Kind:          wrapper.TargetKindMethod,
			ReceiverType:  "Extractor",
			IsPointerRecv: true,
			Parameters: []wrapper.WrapperParam{
				{Name: "_p0", GoType: "int"},
				{Name: "_p1", GoType: "string"},
			},
			HasResult:    true,
			ResultGoType: "string",
			ResultCount:  1,
		},
		{
			ID:         "example.com/blank:AddUnnamed",
			SymbolName: "AddUnnamed",
			Kind:       wrapper.TargetKindFunction,
			Parameters: []wrapper.WrapperParam{
				{Name: "_p0", GoType: "int"},
				{Name: "_p1", GoType: "int"},
			},
			HasResult:    true,
			ResultGoType: "int",
			ResultCount:  1,
		},
	}

	wrapperPath, _, err := wrapper.WriteWrapperFile(wrapperDir, "blank", targets, nil)
	if err != nil {
		t.Fatalf("WriteWrapperFile: %v", err)
	}

	src, err := os.ReadFile(wrapperPath)
	if err != nil {
		t.Fatalf("read wrapper: %v", err)
	}
	// Static guard: the generated file must contain no reference to the
	// blank identifier as a value (parameter local, address-of, or call
	// argument). The patterns below cover the three failure shapes from
	// the bug report.
	bannedPatterns := []string{
		"var _ ",
		"&_)",
		"(_, _)",
	}
	for _, banned := range bannedPatterns {
		if strings.Contains(string(src), banned) {
			t.Errorf("generated wrapper contains banned pattern %q; source:\n%s", banned, src)
		}
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

	cmd := exec.Command("go", "build", "-buildvcs=false", "-overlay", manifestPath, "./...")
	cmd.Dir = modDir
	cmd.Env = append(os.Environ(), "GOFLAGS=")
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		t.Fatalf("go build failed: %v\nstderr: %s\ngenerated wrapper:\n%s",
			err, stderr.String(), src)
	}
	if strings.Contains(stderr.String(), "cannot use _ as value or type") {
		t.Errorf("build emitted blank-identifier diagnostic:\nstderr: %s", stderr.String())
	}
}

// TestGeneratedWrapperCompilesVariadic is the str-jeen.48 regression: a
// wrapper for a function whose final parameter is variadic must emit
// `args...` at the call site. Pre-fix the wrapper emitted `runCommand(ctx,
// binaryPath, args)` for `func(context.Context, string, ...string)`,
// producing a `cannot use args (variable of type []string) as string`
// build error. The fixtures cover the three failure shapes Zolem
// surfaced: a free variadic function, a variadic factory, and a helper
// that takes `...uint64`.
func TestGeneratedWrapperCompilesVariadic(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go binary not found")
	}

	modDir := t.TempDir()
	wrapperDir := t.TempDir()

	const targetSrc = `package variadic

import "context"

// RunCommand is the canonical free variadic function from Zolem.
func RunCommand(ctx context.Context, name string, args ...string) int {
	_ = ctx
	return len(name) + len(args)
}

type Generator struct{ id int }
type Handler struct{ gens []*Generator }

// MakeHandler is the variadic factory shape: NewHandler(...*Generator).
func MakeHandler(gens ...*Generator) *Handler {
	return &Handler{gens: gens}
}

// CallU32 is the WASM-helper shape: callU32(...uint64).
func CallU32(args ...uint64) uint64 {
	var sum uint64
	for _, v := range args {
		sum += v
	}
	return sum
}
`
	if err := os.WriteFile(filepath.Join(modDir, "variadic.go"), []byte(targetSrc), 0o644); err != nil {
		t.Fatalf("write variadic.go: %v", err)
	}
	if err := os.WriteFile(filepath.Join(modDir, "go.mod"), []byte("module example.com/variadic\n\ngo 1.23.0\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	targets := []wrapper.WrapperTarget{
		{
			ID:         "example.com/variadic:RunCommand",
			SymbolName: "RunCommand",
			Kind:       wrapper.TargetKindFunction,
			Parameters: []wrapper.WrapperParam{
				{Name: "ctx", GoType: "context.Context"},
				{Name: "name", GoType: "string"},
				{Name: "args", GoType: "[]string", IsVariadic: true},
			},
			Imports:      []string{"context"},
			HasResult:    true,
			ResultGoType: "int",
			ResultCount:  1,
		},
		{
			ID:         "example.com/variadic:MakeHandler",
			SymbolName: "MakeHandler",
			Kind:       wrapper.TargetKindFunction,
			Parameters: []wrapper.WrapperParam{
				{Name: "gens", GoType: "[]*Generator", IsVariadic: true},
			},
			HasResult:    true,
			ResultGoType: "*Handler",
			ResultCount:  1,
		},
		{
			ID:         "example.com/variadic:CallU32",
			SymbolName: "CallU32",
			Kind:       wrapper.TargetKindFunction,
			Parameters: []wrapper.WrapperParam{
				{Name: "args", GoType: "[]uint64", IsVariadic: true},
			},
			HasResult:    true,
			ResultGoType: "uint64",
			ResultCount:  1,
		},
	}

	wrapperPath, _, err := wrapper.WriteWrapperFile(wrapperDir, "variadic", targets, nil)
	if err != nil {
		t.Fatalf("WriteWrapperFile: %v", err)
	}

	src, err := os.ReadFile(wrapperPath)
	if err != nil {
		t.Fatalf("read wrapper: %v", err)
	}

	mustContain := []string{
		"RunCommand(ctx, name, args...)",
		"MakeHandler(gens...)",
		"CallU32(args...)",
	}
	for _, want := range mustContain {
		if !strings.Contains(string(src), want) {
			t.Errorf("generated wrapper missing variadic call %q\nsource:\n%s", want, src)
		}
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

	cmd := exec.Command("go", "build", "-buildvcs=false", "-overlay", manifestPath, "./...")
	cmd.Dir = modDir
	cmd.Env = append(os.Environ(), "GOFLAGS=")
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		t.Fatalf("go build failed: %v\nstderr: %s\ngenerated wrapper:\n%s",
			err, stderr.String(), src)
	}
}

// TestGeneratedWrapperRespectsConstructorReturnKind is the str-jeen.49
// regression: the wrapper must not blindly dereference a constructor
// result. When `DefaultRegistry()` returns the value type `Registry`,
// emitting `_recv := *DefaultRegistry()` for a value receiver fails
// with `cannot indirect DefaultRegistry() (value of struct type
// Registry)`. The four legal combinations of constructor return kind
// (value vs pointer) and receiver kind (value vs pointer) each have a
// distinct correct call shape, exercised below.
func TestGeneratedWrapperRespectsConstructorReturnKind(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go binary not found")
	}

	modDir := t.TempDir()
	wrapperDir := t.TempDir()

	const targetSrc = `package retkind

type Registry struct{ n int }
type Service struct{ n int }

// DefaultRegistry returns a value type — the str-jeen.49 failure case.
func DefaultRegistry() Registry { return Registry{} }

// NewService returns a pointer type.
func NewService() *Service { return &Service{} }

func (r Registry) Get() int  { return r.n }
func (r *Registry) Inc()     { r.n++ }
func (s Service) GetS() int  { return s.n }
func (s *Service) IncS()     { s.n++ }
`
	if err := os.WriteFile(filepath.Join(modDir, "retkind.go"), []byte(targetSrc), 0o644); err != nil {
		t.Fatalf("write retkind.go: %v", err)
	}
	if err := os.WriteFile(filepath.Join(modDir, "go.mod"), []byte("module example.com/retkind\n\ngo 1.23.0\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	targets := []wrapper.WrapperTarget{
		{
			ID:           "example.com/retkind:(Registry).Get",
			SymbolName:   "Get",
			Kind:         wrapper.TargetKindMethod,
			ReceiverType: "Registry", IsPointerRecv: false,
			HasResult: true, ResultGoType: "int", ResultCount: 1,
		},
		{
			ID:           "example.com/retkind:(*Registry).Inc",
			SymbolName:   "Inc",
			Kind:         wrapper.TargetKindMethod,
			ReceiverType: "Registry", IsPointerRecv: true,
		},
		{
			ID:           "example.com/retkind:(Service).GetS",
			SymbolName:   "GetS",
			Kind:         wrapper.TargetKindMethod,
			ReceiverType: "Service", IsPointerRecv: false,
			HasResult: true, ResultGoType: "int", ResultCount: 1,
		},
		{
			ID:           "example.com/retkind:(*Service).IncS",
			SymbolName:   "IncS",
			Kind:         wrapper.TargetKindMethod,
			ReceiverType: "Service", IsPointerRecv: true,
		},
	}
	ctors := []wrapper.ConstructorCandidate{
		{FuncName: "DefaultRegistry", TargetType: "Registry", ReturnsPointer: false},
		{FuncName: "NewService", TargetType: "Service", ReturnsPointer: true},
	}

	src := wrapper.GenerateWrapper("retkind", targets, ctors)

	// Static guards: the four legal call shapes must each appear, and
	// the bug shape (`*DefaultRegistry()`) must NOT.
	mustContain := []string{
		// value-return constructor + value receiver: direct use.
		"_recv := DefaultRegistry()",
		// pointer-return constructor + pointer receiver: direct use.
		"_recv := NewService()",
		// pointer-return constructor + value receiver: dereference.
		"_recv := *NewService()",
	}
	for _, want := range mustContain {
		if !strings.Contains(src, want) {
			t.Errorf("generated wrapper missing expected line %q\nsource:\n%s", want, src)
		}
	}
	// Banned: indirecting a value-returning constructor.
	if strings.Contains(src, "*DefaultRegistry()") {
		t.Errorf("generated wrapper indirects DefaultRegistry (which returns a value type)\nsource:\n%s", src)
	}
	// Pointer-receiver method on a value-returning constructor: must
	// take the address of a named local, not call &DefaultRegistry().
	if strings.Contains(src, "&DefaultRegistry()") {
		t.Errorf("generated wrapper takes address of DefaultRegistry() return value directly\nsource:\n%s", src)
	}

	wrapperPath, _, err := wrapper.WriteWrapperFile(wrapperDir, "retkind", targets, ctors)
	if err != nil {
		t.Fatalf("WriteWrapperFile: %v", err)
	}
	hash := wrapper.DiscoveryHash(targets, ctors)
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
	cmd := exec.Command("go", "build", "-buildvcs=false", "-overlay", manifestPath, "./...")
	cmd.Dir = modDir
	cmd.Env = append(os.Environ(), "GOFLAGS=")
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		got, _ := os.ReadFile(wrapperPath)
		t.Fatalf("go build failed: %v\nstderr: %s\ngenerated wrapper:\n%s", err, stderr.String(), got)
	}
	if strings.Contains(stderr.String(), "cannot indirect") {
		t.Errorf("build emitted cannot-indirect diagnostic:\nstderr: %s", stderr.String())
	}
}

// TestVariadicWrapperEndToEndFromPackagesLoad is the str-fxj7 regression:
// closes the gap between TestBuildWrapperTargets_DetectsVariadic (AST-only,
// no compile) and TestGeneratedWrapperCompilesVariadic (compiles, but uses
// hand-built WrapperTargets that bypass extractWrapperParams). This test
// drives the full path that production prepare hits — packages.Load →
// BuildWrapperTargets → WriteWrapperFile → `go build` — for the exact
// zolem signatures from str-fxj7:
//
//   - runCommand(ctx context.Context, binaryPath string, args ...string)
//     (unexported free function with leading non-variadic params)
//   - callU32(ctx context.Context, fn Function, args ...uint64)
//     (variadic of a non-primitive type)
//   - (*Server).Send(args ...string) (variadic on a method receiver)
//
// Pre-fix, any path that lost IsVariadic between extraction and call-site
// emission would surface as `cannot use args (variable of type []T) as T
// value in argument to ...`.
func TestVariadicWrapperEndToEndFromPackagesLoad(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go binary not found")
	}

	modDir := t.TempDir()
	wrapperDir := t.TempDir()

	const targetSrc = `package zolemlike

import "context"

// Function mirrors the wazero api.Function interface shape that zolem's
// callU32 takes as its second parameter.
type Function interface {
	Call(ctx context.Context, args ...uint64) ([]uint64, error)
}

// runCommand is the unexported free variadic function from zolem's
// internal/ollama/client.go (str-fxj7).
func runCommand(ctx context.Context, binaryPath string, args ...string) error {
	_ = ctx
	_ = binaryPath
	_ = args
	return nil
}

// callU32 mirrors zolem's internal/wasmgen/generator.go shape: variadic
// uint64 with a leading non-primitive (interface) parameter.
func callU32(ctx context.Context, fn Function, args ...uint64) (uint64, error) {
	_ = ctx
	_ = fn
	var sum uint64
	for _, v := range args {
		sum += v
	}
	return sum, nil
}

type Server struct{}

// Send exercises a variadic on a pointer-receiver method.
func (s *Server) Send(args ...string) int { return len(args) }
`
	if err := os.WriteFile(filepath.Join(modDir, "zolemlike.go"), []byte(targetSrc), 0o644); err != nil {
		t.Fatalf("write source: %v", err)
	}
	if err := os.WriteFile(filepath.Join(modDir, "go.mod"), []byte("module example.com/zolemlike\n\ngo 1.23.0\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	cfg := &packages.Config{
		Mode: packages.NeedName | packages.NeedFiles | packages.NeedSyntax |
			packages.NeedTypes | packages.NeedTypesInfo |
			packages.NeedCompiledGoFiles | packages.NeedImports | packages.NeedDeps,
		Dir: modDir,
	}
	pkgs, err := packages.Load(cfg, ".")
	if err != nil {
		t.Fatalf("packages.Load: %v", err)
	}
	if len(pkgs) != 1 {
		t.Fatalf("expected 1 package, got %d", len(pkgs))
	}
	for _, e := range pkgs[0].Errors {
		t.Fatalf("package load error: %v", e)
	}

	targets := wrapper.BuildWrapperTargets(pkgs[0])

	// Confirm IsVariadic survived extraction for every variadic target.
	wantVariadic := map[string]string{
		"runCommand": "args",
		"callU32":    "args",
		"Send":       "args",
	}
	for _, tg := range targets {
		paramName, expected := wantVariadic[tg.SymbolName]
		if !expected {
			continue
		}
		if len(tg.Parameters) == 0 {
			t.Errorf("%s: no parameters", tg.SymbolName)
			continue
		}
		last := tg.Parameters[len(tg.Parameters)-1]
		if last.Name != paramName {
			t.Errorf("%s: last param name = %q, want %q", tg.SymbolName, last.Name, paramName)
		}
		if !last.IsVariadic {
			t.Errorf("%s: last param IsVariadic=false (extraction dropped variadic flag)", tg.SymbolName)
		}
	}

	wrapperPath, _, err := wrapper.WriteWrapperFile(wrapperDir, "zolemlike", targets, nil)
	if err != nil {
		t.Fatalf("WriteWrapperFile: %v", err)
	}
	src, err := os.ReadFile(wrapperPath)
	if err != nil {
		t.Fatalf("read wrapper: %v", err)
	}

	mustContain := []string{
		"runCommand(ctx, binaryPath, args...)",
		"callU32(ctx, fn, args...)",
		"_recv.Send(args...)",
	}
	for _, want := range mustContain {
		if !strings.Contains(string(src), want) {
			t.Errorf("generated wrapper missing variadic call %q\nsource:\n%s", want, src)
		}
	}

	// Stage the wrapper file in-tree via the overlay manifest so the wrapper
	// compiles against the unexported targets in the same package.
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

	cmd := exec.Command("go", "build", "-buildvcs=false", "-overlay", manifestPath, "./...")
	cmd.Dir = modDir
	cmd.Env = append(os.Environ(), "GOFLAGS=")
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		t.Fatalf("go build failed: %v\nstderr: %s\ngenerated wrapper:\n%s",
			err, stderr.String(), src)
	}
}

// TestPointerWrapperEndToEndFromPackagesLoad is the str-9j2e regression. It
// drives the full path that production prepare hits — packages.Load →
// BuildWrapperTargets → WriteWrapperFile → `go build` — for the two zolem
// signatures captured in the bug:
//
//   - A free function with a non-variadic pointer parameter — the
//     `internal/provider/openai/handler.go::NewHandler(gen *wasmgen.Generator)`
//     shape. Pre-fix surfaced as `cannot use wasmGenerator (variable of type
//     []*wasmgen.Generator) as *wasmgen.Generator value` whenever a path
//     between extraction and the call site re-injected the variadic slice
//     prefix.
//   - A value-returning constructor combined with a value-receiver method —
//     the `internal/specs/registry.go::DefaultRegistry` shape. Pre-fix
//     surfaced as `cannot indirect DefaultRegistry() (value of struct type
//     Registry)` because the wrapper applied a pointer dereference to a
//     value-typed expression.
//
// Both shapes are exercised inside one Go module so a single `go build`
// validates the wrapper compiles for both. Because BuildWrapperTargets and
// ScanConstructors are the same path the production prepare hits, any
// regression that breaks pointer shape or constructor return-kind handling
// will fail the build step here.
func TestPointerWrapperEndToEndFromPackagesLoad(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go binary not found")
	}

	modDir := t.TempDir()
	wrapperDir := t.TempDir()

	const targetSrc = `package zolem9j2e

// Generator stands in for an external pointer-typed parameter like
// wasmgen.Generator: a struct passed by pointer to NewHandler.
type Generator struct{ id int }

// Handler is the return type of NewHandler.
type Handler struct{ gen *Generator }

// NewHandler is the str-9j2e pointer-parameter shape from
// internal/provider/openai/handler.go: a non-variadic *T parameter
// must reach the wrapper as ` + "`*T`" + `, never as ` + "`[]*T`" + `.
func NewHandler(wasmGenerator *Generator) *Handler {
	return &Handler{gen: wasmGenerator}
}

// Registry is the str-9j2e value-returning constructor target type.
type Registry struct{ n int }

// DefaultRegistry returns Registry (value, not pointer). Wrapper
// generation must not emit ` + "`*DefaultRegistry()`" + ` for the value
// receiver case below; the value-returning kind has to propagate
// through ScanConstructors all the way to GenerateWrapper.
func DefaultRegistry() Registry { return Registry{} }

// Get is a value-receiver method. The wrapper case for Get must bind
// _recv directly from DefaultRegistry() without an intervening
// pointer dereference, otherwise the package-level build fails with
// ` + "`cannot indirect DefaultRegistry()`" + `.
func (r Registry) Get() int { return r.n }
`
	if err := os.WriteFile(filepath.Join(modDir, "zolem9j2e.go"), []byte(targetSrc), 0o644); err != nil {
		t.Fatalf("write source: %v", err)
	}
	if err := os.WriteFile(filepath.Join(modDir, "go.mod"), []byte("module example.com/zolem9j2e\n\ngo 1.23.0\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	cfg := &packages.Config{
		Mode: packages.NeedName | packages.NeedFiles | packages.NeedSyntax |
			packages.NeedTypes | packages.NeedTypesInfo |
			packages.NeedCompiledGoFiles | packages.NeedImports | packages.NeedDeps,
		Dir: modDir,
	}
	pkgs, err := packages.Load(cfg, ".")
	if err != nil {
		t.Fatalf("packages.Load: %v", err)
	}
	if len(pkgs) != 1 {
		t.Fatalf("expected 1 package, got %d", len(pkgs))
	}
	for _, e := range pkgs[0].Errors {
		t.Fatalf("package load error: %v", e)
	}

	targets := wrapper.BuildWrapperTargets(pkgs[0])

	// Confirm NewHandler's pointer parameter survived as *Generator.
	var newHandler *wrapper.WrapperTarget
	for i, tg := range targets {
		if tg.SymbolName == "NewHandler" {
			newHandler = &targets[i]
			break
		}
	}
	if newHandler == nil {
		t.Fatalf("NewHandler target not found")
	}
	if len(newHandler.Parameters) != 1 {
		t.Fatalf("NewHandler param count = %d, want 1", len(newHandler.Parameters))
	}
	param := newHandler.Parameters[0]
	if param.IsVariadic {
		t.Errorf("NewHandler.Parameters[0].IsVariadic = true (non-variadic pointer mislabeled)")
	}
	if param.GoType != "*Generator" {
		t.Errorf("NewHandler.Parameters[0].GoType = %q, want %q (zolem str-9j2e regression: pointer parameter mis-rendered as slice)", param.GoType, "*Generator")
	}

	// Build the constructor candidate set via the same shape the production
	// scanner produces (protocol.ScanConstructors → toWrapperConstructors).
	// Inlined here to avoid pulling the protocol package into wrapper_test.
	ctors := scanConstructorCandidatesForTest(t, pkgs[0])

	// Confirm DefaultRegistry was classified as a value-returning ctor.
	var hadDefaultRegistry bool
	for _, c := range ctors {
		if c.FuncName == "DefaultRegistry" {
			hadDefaultRegistry = true
			if c.ReturnsPointer {
				t.Errorf("DefaultRegistry classified ReturnsPointer=true (source returns the value type Registry; wrapper would emit *DefaultRegistry() and fail to compile)")
			}
			if c.TargetType != "Registry" {
				t.Errorf("DefaultRegistry.TargetType = %q, want %q", c.TargetType, "Registry")
			}
		}
	}
	if !hadDefaultRegistry {
		t.Fatalf("DefaultRegistry not surfaced by constructor scan; ctors: %+v", ctors)
	}

	src := wrapper.GenerateWrapper("zolem9j2e", targets, ctors)

	// Static guards on the wrapper source.
	mustContain := []string{
		// NewHandler must receive a single *Generator, not a slice.
		"var wasmGenerator *Generator",
		// Registry.Get under the DefaultRegistry receiver-kind: direct
		// value-bind, no pointer dereference.
		"_recv := DefaultRegistry()",
	}
	for _, want := range mustContain {
		if !strings.Contains(src, want) {
			t.Errorf("generated wrapper missing %q\nsource:\n%s", want, src)
		}
	}
	bannedSubstrings := []string{
		// Pointer dereference on value-returning constructor.
		"*DefaultRegistry()",
		// Slice declaration for the non-variadic pointer parameter.
		"var wasmGenerator []*Generator",
	}
	for _, banned := range bannedSubstrings {
		if strings.Contains(src, banned) {
			t.Errorf("generated wrapper contains banned substring %q\nsource:\n%s", banned, src)
		}
	}

	wrapperPath, _, err := wrapper.WriteWrapperFile(wrapperDir, "zolem9j2e", targets, ctors)
	if err != nil {
		t.Fatalf("WriteWrapperFile: %v", err)
	}

	hash := wrapper.DiscoveryHash(targets, ctors)
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

	cmd := exec.Command("go", "build", "-buildvcs=false", "-overlay", manifestPath, "./...")
	cmd.Dir = modDir
	cmd.Env = append(os.Environ(), "GOFLAGS=")
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		t.Fatalf("go build failed: %v\nstderr: %s\ngenerated wrapper:\n%s",
			err, stderr.String(), src)
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

// scanConstructorCandidatesForTest mirrors the production scanner
// (protocol.ScanConstructors + toWrapperConstructors) using only the AST
// and types information that packages.Load populates. It scans the package
// for top-level functions whose return shape is a same-package named type
// (T or *T, optionally paired with error), and whose name starts with
// `New`, `MustNew`, or `Default`, OR whose body contains a composite
// literal return. The returned wrapper.ConstructorCandidate values match
// the production wire shape — in particular, ReturnsPointer carries the
// pointer-ness of the first return so the wrapper case logic in
// str-jeen.49 / str-9j2e can branch correctly.
// TestVariadicForwardingWithIntermediateSlice is the str-jeen.76 regression:
// when a package defines both a variadic helper (chipsHint ...string) and a
// target function that builds a []string and passes it to the helper, the
// generated wrapper for chipsHint must forward the slice with `...` at the
// call site (chipsHint(choices...)). Pre-fix observed in api/internal/fit
// where 8 files failed with "cannot use choices (variable of type []string)
// as string value in argument to chipsHint".
func TestVariadicForwardingWithIntermediateSlice(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go binary not found")
	}

	modDir := t.TempDir()
	wrapperDir := t.TempDir()

	const targetSrc = `package fit

// chipsHint is the variadic helper whose wrapper must use chipsHint(choices...).
func chipsHint(choices ...string) string {
	if len(choices) == 0 {
		return ""
	}
	return choices[0]
}

// CostFit builds an intermediate []string and passes it to chipsHint via spread.
// The wrapper for CostFit itself needs no special handling; the wrapper for
// chipsHint must forward its []string param with "..." to avoid the
// "cannot use choices (variable of type []string) as string value" error.
func CostFit(x int) string {
	choices := []string{"low", "medium", "high"}
	return chipsHint(choices...)
}
`
	if err := os.WriteFile(filepath.Join(modDir, "fit.go"), []byte(targetSrc), 0o644); err != nil {
		t.Fatalf("write fit.go: %v", err)
	}
	if err := os.WriteFile(filepath.Join(modDir, "go.mod"), []byte("module example.com/fit\n\ngo 1.23.0\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}

	cfg := &packages.Config{
		Mode: packages.NeedName | packages.NeedFiles | packages.NeedSyntax |
			packages.NeedTypes | packages.NeedTypesInfo |
			packages.NeedCompiledGoFiles | packages.NeedImports | packages.NeedDeps,
		Dir: modDir,
	}
	pkgs, err := packages.Load(cfg, ".")
	if err != nil {
		t.Fatalf("packages.Load: %v", err)
	}
	if len(pkgs) != 1 {
		t.Fatalf("expected 1 package, got %d", len(pkgs))
	}
	for _, e := range pkgs[0].Errors {
		t.Fatalf("package load error: %v", e)
	}

	targets := wrapper.BuildWrapperTargets(pkgs[0])

	// Confirm chipsHint's variadic parameter survived extraction.
	var chipsHintTarget *wrapper.WrapperTarget
	for i, tg := range targets {
		if tg.SymbolName == "chipsHint" {
			chipsHintTarget = &targets[i]
			break
		}
	}
	if chipsHintTarget == nil {
		t.Fatalf("chipsHint target not found; targets: %+v", targets)
	}
	if len(chipsHintTarget.Parameters) != 1 {
		t.Fatalf("chipsHint: expected 1 parameter, got %d", len(chipsHintTarget.Parameters))
	}
	param := chipsHintTarget.Parameters[0]
	if !param.IsVariadic {
		t.Errorf("chipsHint.Parameters[0].IsVariadic = false, want true (variadic detection regression)")
	}
	if param.GoType != "[]string" {
		t.Errorf("chipsHint.Parameters[0].GoType = %q, want %q", param.GoType, "[]string")
	}

	src := wrapper.GenerateWrapper("fit", targets, nil)
	// The generated call must spread the slice.
	if !strings.Contains(src, "chipsHint(choices...)") {
		t.Errorf("generated wrapper missing variadic spread call chipsHint(choices...)\nsource:\n%s", src)
	}

	// Verify the generated wrapper compiles against the unexported package targets.
	wrapperPath, _, err := wrapper.WriteWrapperFile(wrapperDir, "fit", targets, nil)
	if err != nil {
		t.Fatalf("WriteWrapperFile: %v", err)
	}
	got, _ := os.ReadFile(wrapperPath)

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

	cmd := exec.Command("go", "build", "-buildvcs=false", "-overlay", manifestPath, "./...")
	cmd.Dir = modDir
	cmd.Env = append(os.Environ(), "GOFLAGS=")
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	if err := cmd.Run(); err != nil {
		t.Fatalf("go build failed: %v\nstderr: %s\ngenerated wrapper:\n%s",
			err, stderr.String(), got)
	}
}

func scanConstructorCandidatesForTest(t *testing.T, pkg *packages.Package) []wrapper.ConstructorCandidate {
	t.Helper()
	if pkg == nil || pkg.TypesInfo == nil {
		t.Fatalf("scanConstructorCandidatesForTest: pkg.TypesInfo is nil")
	}
	var ctors []wrapper.ConstructorCandidate
	for _, file := range pkg.Syntax {
		if file == nil {
			continue
		}
		for _, decl := range file.Decls {
			fn, ok := decl.(*ast.FuncDecl)
			if !ok || fn.Body == nil {
				continue
			}
			if fn.Recv != nil && len(fn.Recv.List) > 0 {
				continue
			}
			name := fn.Name.Name
			results := fn.Type.Results
			if results == nil || len(results.List) == 0 {
				continue
			}
			if !ctorNameAllowed(name) && !bodyHasCompositeReturn(fn) {
				continue
			}
			// Flatten the first return field into a single type expression.
			firstField := results.List[0]
			typ := firstField.Type
			returnsPointer := false
			if star, ok := typ.(*ast.StarExpr); ok {
				returnsPointer = true
				typ = star.X
			}
			ident, ok := typ.(*ast.Ident)
			if !ok {
				continue
			}
			obj := pkg.TypesInfo.Uses[ident]
			if obj == nil {
				obj = pkg.TypesInfo.Defs[ident]
			}
			if obj == nil || obj.Pkg() == nil || obj.Pkg().Path() != pkg.PkgPath {
				continue
			}
			ctors = append(ctors, wrapper.ConstructorCandidate{
				FuncName:       name,
				TargetType:     obj.Name(),
				HasParams:      fn.Type.Params != nil && len(fn.Type.Params.List) > 0,
				ReturnsPointer: returnsPointer,
			})
		}
	}
	return ctors
}

func ctorNameAllowed(name string) bool {
	return strings.HasPrefix(name, "New") ||
		strings.HasPrefix(name, "MustNew") ||
		strings.HasPrefix(name, "Default")
}

func bodyHasCompositeReturn(fn *ast.FuncDecl) bool {
	if fn.Body == nil {
		return false
	}
	for _, stmt := range fn.Body.List {
		ret, ok := stmt.(*ast.ReturnStmt)
		if !ok {
			continue
		}
		for _, result := range ret.Results {
			switch e := result.(type) {
			case *ast.CompositeLit:
				return true
			case *ast.UnaryExpr:
				if e.Op == token.AND {
					if _, ok := e.X.(*ast.CompositeLit); ok {
						return true
					}
				}
			}
		}
	}
	return false
}
