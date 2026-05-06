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
// double-emitting `encoding/json`, `fmt`, or `strings` when a target's
// Imports list happens to include them — collectExtraImports filters them.
func TestGenerateWrapperOmitsCoreImportsFromTargetImports(t *testing.T) {
	targets := []wrapper.WrapperTarget{
		{
			ID:         "example.com/dup:F",
			SymbolName: "F",
			Kind:       wrapper.TargetKindFunction,
			Parameters: []wrapper.WrapperParam{{Name: "x", GoType: "int"}},
			// Deliberately include the always-emitted core imports.
			Imports:      []string{"encoding/json", "fmt", "strings", "context"},
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
