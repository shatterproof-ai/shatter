package config_test

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/config"
)

// writeConfig writes the supplied YAML body into a fresh temporary
// .shatter/config.yaml and returns the path to a sibling source file the
// loader can use as a starting point for its upward walk.
func writeConfig(t *testing.T, body string) string {
	t.Helper()
	dir := t.TempDir()
	shatterDir := filepath.Join(dir, ".shatter")
	if err := os.MkdirAll(shatterDir, 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	if err := os.WriteFile(filepath.Join(shatterDir, "config.yaml"), []byte(body), 0o644); err != nil {
		t.Fatalf("write config: %v", err)
	}
	target := filepath.Join(dir, "target.go")
	if err := os.WriteFile(target, []byte("package x"), 0o644); err != nil {
		t.Fatalf("write target: %v", err)
	}
	return target
}

func TestLoad_DefaultsSection_LiteralAndTypeHint(t *testing.T) {
	t.Parallel()
	target := writeConfig(t, `
functions:
  "target.go:UseDefaults":
    defaults:
      name: "alice"
      age: 42
      ratio: 1.5
      enabled: true
`)
	file, err := config.Load(target)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if len(file.Warnings) != 0 {
		t.Fatalf("expected no warnings, got %v", file.Warnings)
	}
	entry := file.MatchTarget("target.go", "UseDefaults")
	if len(entry.Defaults) != 4 {
		t.Fatalf("expected 4 defaults, got %d (%+v)", len(entry.Defaults), entry.Defaults)
	}
	cases := []struct {
		paramName string
		wantJSON  string
		wantHint  string
	}{
		{"name", `"alice"`, "string"},
		{"age", `42`, "int"},
		{"ratio", `1.5`, "float64"},
		{"enabled", `true`, "bool"},
	}
	for _, tc := range cases {
		got, ok := entry.Defaults[tc.paramName]
		if !ok {
			t.Errorf("default %q missing", tc.paramName)
			continue
		}
		if string(got.JSON) != tc.wantJSON {
			t.Errorf("default %q JSON = %s, want %s", tc.paramName, string(got.JSON), tc.wantJSON)
		}
		if got.TypeHint != tc.wantHint {
			t.Errorf("default %q TypeHint = %q, want %q", tc.paramName, got.TypeHint, tc.wantHint)
		}
		if !json.Valid(got.JSON) {
			t.Errorf("default %q JSON is not valid: %s", tc.paramName, string(got.JSON))
		}
	}
}

func TestLoad_MocksSection(t *testing.T) {
	t.Parallel()
	target := writeConfig(t, `
functions:
  "target.go:UsesFmt":
    mocks:
      "fmt.Println": "func(a ...any) (int, error) { return 0, nil }"
      "time.Now": "func() time.Time { return time.Time{} }"
`)
	file, err := config.Load(target)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if len(file.Warnings) != 0 {
		t.Fatalf("unexpected warnings: %v", file.Warnings)
	}
	entry := file.MatchTarget("target.go", "UsesFmt")
	if got := entry.Mocks["fmt.Println"]; !strings.Contains(got, "return 0, nil") {
		t.Errorf("fmt.Println mock = %q, want substring \"return 0, nil\"", got)
	}
	if got := entry.Mocks["time.Now"]; !strings.Contains(got, "time.Time{}") {
		t.Errorf("time.Now mock = %q, want substring \"time.Time{}\"", got)
	}
}

func TestLoad_GeneratorsSection(t *testing.T) {
	t.Parallel()
	target := writeConfig(t, `
functions:
  "target.go:UsesCtx":
    generators:
      ctx: context.Context
      buf: "*bytes.Buffer"
`)
	file, err := config.Load(target)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if len(file.Warnings) != 0 {
		t.Fatalf("unexpected warnings: %v", file.Warnings)
	}
	entry := file.MatchTarget("target.go", "UsesCtx")
	if entry.Generators["ctx"] != "context.Context" {
		t.Errorf("generator ctx = %q, want context.Context", entry.Generators["ctx"])
	}
	if entry.Generators["buf"] != "*bytes.Buffer" {
		t.Errorf("generator buf = %q, want *bytes.Buffer", entry.Generators["buf"])
	}
}

func TestLoad_GoRuntimeValuesSection(t *testing.T) {
	t.Parallel()
	target := writeConfig(t, `
go_runtime_values:
  "fixture.CompiledModule":
    expression: |
      func() fixture.CompiledModule {
        return fixture.CompiledModule{}
      }()
    imports:
      - context
      - zolem.dev/zolem/internal/fixture
`)
	file, err := config.Load(target)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if len(file.Warnings) != 0 {
		t.Fatalf("unexpected warnings: %v", file.Warnings)
	}
	rv, ok := file.GoRuntimeValues["fixture.CompiledModule"]
	if !ok {
		t.Fatalf("GoRuntimeValues missing fixture.CompiledModule: %+v", file.GoRuntimeValues)
	}
	if !strings.Contains(rv.Expression, "return fixture.CompiledModule{}") {
		t.Errorf("Expression = %q, want configured Go expression", rv.Expression)
	}
	wantImports := []string{"context", "zolem.dev/zolem/internal/fixture"}
	if len(rv.Imports) != len(wantImports) {
		t.Fatalf("Imports = %v, want %v", rv.Imports, wantImports)
	}
	for i, want := range wantImports {
		if rv.Imports[i] != want {
			t.Errorf("Imports[%d] = %q, want %q", i, rv.Imports[i], want)
		}
	}
}

// AC4 — unknown keys must warn without failing. Both top-level and
// per-function unknown keys are surfaced through File.Warnings.
func TestLoad_UnknownKeys_WarnButNotFail(t *testing.T) {
	t.Parallel()
	target := writeConfig(t, `
made_up_top: 1
functions:
  "target.go:Sample":
    policy:
      allow: [database]
    typo_section:
      foo: bar
    another_typo: 42
`)
	file, err := config.Load(target)
	if err != nil {
		t.Fatalf("Load returned error for unknown keys (must warn instead): %v", err)
	}
	// The known section still parses.
	entry := file.MatchTarget("target.go", "Sample")
	if entry.Policy == nil || len(entry.Policy.Allow) != 1 || entry.Policy.Allow[0] != "database" {
		t.Errorf("policy.allow not preserved across unknown keys: %+v", entry.Policy)
	}
	// And warnings are emitted for both top-level and nested unknowns.
	joined := strings.Join(file.Warnings, "\n")
	if !strings.Contains(joined, "unknown top-level key \"made_up_top\"") {
		t.Errorf("missing top-level warning, got:\n%s", joined)
	}
	if !strings.Contains(joined, `function "target.go:Sample"`) || !strings.Contains(joined, `unknown key "typo_section"`) {
		t.Errorf("missing function-key warning for typo_section, got:\n%s", joined)
	}
	if !strings.Contains(joined, `unknown key "another_typo"`) {
		t.Errorf("missing function-key warning for another_typo, got:\n%s", joined)
	}
}

// AC4 also requires the existing most-specific-match-wins semantics to be
// preserved. This test exercises the matcher across the new sections.
func TestLoad_MostSpecificMatchWins_AcrossSections(t *testing.T) {
	t.Parallel()
	target := writeConfig(t, `
functions:
  "*:*":
    defaults:
      name: "wildcard"
  "target.go:Pick":
    defaults:
      name: "specific"
    mocks:
      "fmt.Println": "noop"
`)
	file, err := config.Load(target)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	entry := file.MatchTarget("target.go", "Pick")
	if got := entry.Defaults["name"]; string(got.JSON) != `"specific"` {
		t.Errorf("specific defaults.name = %s, want \"specific\"", string(got.JSON))
	}
	if entry.Mocks["fmt.Println"] != "noop" {
		t.Errorf("specific mock missing: %v", entry.Mocks)
	}
	// Pattern that only matches the wildcard still resolves.
	wild := file.MatchTarget("target.go", "Other")
	if got := wild.Defaults["name"]; string(got.JSON) != `"wildcard"` {
		t.Errorf("wildcard defaults.name = %s, want \"wildcard\"", string(got.JSON))
	}
}

// AC5 — defaults take priority over classifyParamFamily defaults inside
// PlanParam. The loader test cannot import the planner, but the contract is
// that DefaultValue.JSON is a valid ValuePlan literal and DefaultValue.TypeHint
// is the Go type spelling. This test pins the encoding so the planner-side
// hookup stays sound.
func TestLoad_DefaultPrecedenceContract_LiteralIsPlannerReady(t *testing.T) {
	t.Parallel()
	target := writeConfig(t, `
functions:
  "target.go:Greet":
    defaults:
      who: "world"
      n: 7
`)
	file, err := config.Load(target)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	entry := file.MatchTarget("target.go", "Greet")

	who := entry.Defaults["who"]
	if !bytes.Equal(who.JSON, []byte(`"world"`)) {
		t.Errorf("who.JSON = %s, want \"world\"", string(who.JSON))
	}
	if who.TypeHint != "string" {
		t.Errorf("who.TypeHint = %q, want string", who.TypeHint)
	}

	n := entry.Defaults["n"]
	if !bytes.Equal(n.JSON, []byte(`7`)) {
		t.Errorf("n.JSON = %s, want 7", string(n.JSON))
	}
	if n.TypeHint != "int" {
		t.Errorf("n.TypeHint = %q, want int", n.TypeHint)
	}
}

// str-rd0a: the hint-config resolver (shatter-go/main.go) historically passed
// the raw FunctionAnalysis.SourceFile — an ABSOLUTE path during scans — to
// MatchTarget, while the policy resolver normalized it first. filepath.Match
// never matches a basename pattern against an absolute path, so per-function
// `defaults`/`generators` globs silently failed for hints while working for
// policy. config.TargetRelpath centralizes the normalization both paths must use.
func TestTargetRelpath_NormalizesAbsoluteToBasename(t *testing.T) {
	t.Parallel()
	if got := config.TargetRelpath("/abs/module/internal/fixture/loader.go"); got != "loader.go" {
		t.Errorf("TargetRelpath(absolute) = %q, want %q", got, "loader.go")
	}
	if got := config.TargetRelpath("internal/fixture/loader.go"); got != "internal/fixture/loader.go" {
		t.Errorf("TargetRelpath(relative) = %q, want it unchanged", got)
	}
}

func TestMatchTarget_AbsoluteSourceFileMatchesViaTargetRelpath(t *testing.T) {
	t.Parallel()
	target := writeConfig(t, `
functions:
  "loader.go:loadOne":
    defaults:
      dir: "/fixtures/sample"
`)
	file, err := config.Load(target)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	absPath := "/home/user/project/internal/fixture/loader.go"
	// The bug: the raw absolute SourceFile does not match a basename key.
	if got := file.MatchTarget(absPath, "loadOne"); len(got.Defaults) != 0 {
		t.Fatalf("raw absolute path unexpectedly matched (defaults=%+v)", got.Defaults)
	}
	// The fix: normalizing the SourceFile via TargetRelpath matches.
	entry := file.MatchTarget(config.TargetRelpath(absPath), "loadOne")
	if len(entry.Defaults) != 1 {
		t.Fatalf("normalized absolute path failed to match defaults: %+v", entry.Defaults)
	}
	if !bytes.Equal(entry.Defaults["dir"].JSON, []byte(`"/fixtures/sample"`)) {
		t.Errorf("dir default = %s, want \"/fixtures/sample\"", string(entry.Defaults["dir"].JSON))
	}
}

func TestLoad_MissingFile_ReturnsZeroFile(t *testing.T) {
	t.Parallel()
	dir := t.TempDir()
	target := filepath.Join(dir, "x.go")
	if err := os.WriteFile(target, []byte("package x"), 0o644); err != nil {
		t.Fatal(err)
	}
	file, err := config.Load(target)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if len(file.Functions) != 0 || len(file.Warnings) != 0 {
		t.Errorf("expected empty File, got %+v", file)
	}
}
