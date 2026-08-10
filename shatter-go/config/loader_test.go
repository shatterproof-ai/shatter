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

func TestLoad_SeedsSection_PoolOfDocuments(t *testing.T) {
	t.Parallel()
	target := writeConfig(t, `
functions:
  "target.go:UseSeeds":
    seeds:
      data:
        - openapi: "3.0.0"
          info:
            title: "a"
        - openapi: "3.1.0"
          info:
            title: "b"
`)
	file, err := config.Load(target)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if len(file.Warnings) != 0 {
		t.Fatalf("expected no warnings, got %v", file.Warnings)
	}
	entry := file.MatchTarget("target.go", "UseSeeds")
	docs, ok := entry.Seeds["data"]
	if !ok {
		t.Fatalf("seeds[%q] missing", "data")
	}
	if len(docs) != 2 {
		t.Fatalf("expected 2 seed documents, got %d (%+v)", len(docs), docs)
	}
	for i, doc := range docs {
		if !json.Valid(doc.JSON) {
			t.Errorf("seed %d JSON is not valid: %s", i, string(doc.JSON))
		}
		var decoded map[string]any
		if err := json.Unmarshal(doc.JSON, &decoded); err != nil {
			t.Errorf("seed %d did not decode as an object: %v", i, err)
		}
		if v, ok := decoded["openapi"]; !ok || v == "" {
			t.Errorf("seed %d missing openapi field: %s", i, string(doc.JSON))
		}
	}
	if !strings.Contains(string(docs[0].JSON), `"3.0.0"`) {
		t.Errorf("seed 0 JSON = %s, want to contain 3.0.0", string(docs[0].JSON))
	}
	if !strings.Contains(string(docs[1].JSON), `"3.1.0"`) {
		t.Errorf("seed 1 JSON = %s, want to contain 3.1.0", string(docs[1].JSON))
	}
}

func TestLoad_SeedsSection_UnknownKeyNotWarned(t *testing.T) {
	t.Parallel()
	target := writeConfig(t, `
functions:
  "target.go:UseSeeds":
    seeds:
      data:
        - {}
`)
	file, err := config.Load(target)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if len(file.Warnings) != 0 {
		t.Fatalf("seeds is a known key; expected no warnings, got %v", file.Warnings)
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
	if got := entry.Mocks["fmt.Println"].Expression; !strings.Contains(got, "return 0, nil") {
		t.Errorf("fmt.Println mock = %q, want substring \"return 0, nil\"", got)
	}
	if got := entry.Mocks["time.Now"].Expression; !strings.Contains(got, "time.Time{}") {
		t.Errorf("time.Now mock = %q, want substring \"time.Time{}\"", got)
	}
}

// str-7lab0: one YAML file feeds both this loader and the Rust CLI, whose
// schema is struct-shaped ({expression, return_values, behavior}). Both the
// bare-string shorthand and the struct form must parse here; CLI-owned keys
// are tolerated, and a struct entry without an expression yields the empty
// string (skipped by every downstream consumer).
func TestLoad_MocksDualForm(t *testing.T) {
	t.Parallel()
	target := writeConfig(t, `
functions:
  "target.go:*":
    mocks:
      "auth.GetAccount": "auth.StaticAccount()"
      "svc.Fetch":
        expression: "svc.Fake()"
      "db.Query":
        return_values:
          - {"rows": []}
        behavior: repeat_last
`)
	file, err := config.Load(target)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	entry := file.MatchTarget("target.go", "Anything")
	if got := entry.Mocks["auth.GetAccount"].Expression; got != "auth.StaticAccount()" {
		t.Errorf("bare-string mock = %q, want auth.StaticAccount()", got)
	}
	if got := entry.Mocks["svc.Fetch"].Expression; got != "svc.Fake()" {
		t.Errorf("struct-form expression = %q, want svc.Fake()", got)
	}
	if got := entry.Mocks["db.Query"].Expression; got != "" {
		t.Errorf("CLI-owned struct entry should parse to empty expression, got %q", got)
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

func TestLoad_ReceiverSection(t *testing.T) {
	t.Parallel()
	target := writeConfig(t, `
functions:
  "target.go:(*Service).Run":
    receiver:
      label: seeded_service
      expression: |
        &Service{backend: fakeBackend{}}
      imports:
        - example.com/project/internal/fakes
`)
	file, err := config.Load(target)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if len(file.Warnings) != 0 {
		t.Fatalf("unexpected warnings: %v", file.Warnings)
	}
	entry := file.MatchTarget("target.go", "(*Service).Run")
	if entry.Receiver == nil {
		t.Fatal("receiver config missing")
	}
	if entry.Receiver.Label != "seeded_service" {
		t.Errorf("receiver label = %q, want seeded_service", entry.Receiver.Label)
	}
	if !strings.Contains(entry.Receiver.Expression, "&Service{backend: fakeBackend{}}") {
		t.Errorf("receiver expression = %q, want configured expression", entry.Receiver.Expression)
	}
	if got, want := entry.Receiver.Imports, []string{"example.com/project/internal/fakes"}; len(got) != len(want) || got[0] != want[0] {
		t.Errorf("receiver imports = %v, want %v", got, want)
	}
	if got := entry.Receiver.ReceiverKind(); got != "configured:seeded_service" {
		t.Errorf("receiver kind = %q, want configured:seeded_service", got)
	}
}

func TestLoad_ReceiverSectionMissingExpressionWarns(t *testing.T) {
	t.Parallel()
	target := writeConfig(t, `
functions:
  "target.go:(*Service).Run":
    receiver:
      label: seeded_service
  "target.go:(*Service).Stop":
    receiver:
      expression: "   "
`)
	file, err := config.Load(target)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	joined := strings.Join(file.Warnings, "\n")
	for _, pattern := range []string{
		`function "target.go:(*Service).Run": receiver expression is empty`,
		`function "target.go:(*Service).Stop": receiver expression is empty`,
	} {
		if !strings.Contains(joined, pattern) {
			t.Errorf("missing receiver warning %q, got:\n%s", pattern, joined)
		}
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
	if entry.Mocks["fmt.Println"].Expression != "noop" {
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
	// A filename-scoped key now resolves the same way whichever spelling the
	// caller holds. The raw absolute path matches via the basename fallback in
	// matchFileGlob (it did not before that fallback existed); the
	// TargetRelpath-normalized form matches directly. TargetRelpath is still
	// the required normalization — it also collapses "../"-escaping paths and
	// is what anchored, path-scoped keys are matched against.
	for label, spelling := range map[string]string{
		"raw absolute":         absPath,
		"TargetRelpath-lized":  config.TargetRelpath(absPath),
		"repo-relative nested": "internal/fixture/loader.go",
	} {
		entry := file.MatchTarget(spelling, "loadOne")
		if len(entry.Defaults) != 1 {
			t.Fatalf("%s path failed to match defaults: %+v", label, entry.Defaults)
		}
		if !bytes.Equal(entry.Defaults["dir"].JSON, []byte(`"/fixtures/sample"`)) {
			t.Errorf("%s: dir default = %s, want \"/fixtures/sample\"", label, string(entry.Defaults["dir"].JSON))
		}
	}
}

// Filename-scoped globs must match a nested *relative* source path, not just an
// absolute one. TargetRelpath collapses absolute paths to their basename, so
// "*.resolvers.go:*" matched when the frontend happened to hold an absolute
// SourceFile (the planner/hint path) but silently failed when it held a clean
// repo-relative path (the prepare/execute path) — filepath.Match's "*" never
// crosses a separator. The asymmetry made `mocks` entries resolve for planning
// and vanish at execute time, so config mock expressions were never substituted
// (kapow-jdb8: every gqlgen resolver kept hitting its real auth gate).
func TestMatchTarget_FilenameGlobMatchesNestedRelativePath(t *testing.T) {
	t.Parallel()
	target := writeConfig(t, `
functions:
  "*.resolvers.go:*":
    mocks:
      "auth.GetAccount": "auth.StaticAccount()"
  "resolver.go:*":
    mocks:
      "auth.GetAccount": "auth.StaticAccount()"
`)
	file, err := config.Load(target)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	for _, relpath := range []string{
		"api/graph/resolver/auth.resolvers.go", // filename glob, nested relative
		"auth.resolvers.go",                    // filename glob, already a basename
		"api/graph/resolver/resolver.go",       // exact basename literal, nested
	} {
		entry := file.MatchTarget(config.TargetRelpath(relpath), "CreateTeam")
		if len(entry.Mocks) != 1 {
			t.Errorf("relpath %q: got %d mocks, want 1", relpath, len(entry.Mocks))
			continue
		}
		if got := entry.Mocks["auth.GetAccount"].Expression; got != "auth.StaticAccount()" {
			t.Errorf("relpath %q: expression = %q, want %q", relpath, got, "auth.StaticAccount()")
		}
	}
}

// The basename fallback applies only to filename-scoped globs. A pattern that
// carries a path separator stays anchored to the full relative path, so it must
// not start matching a bare basename (or a same-named file in another
// directory) as a side effect of the fix above.
func TestMatchTarget_PathScopedGlobStaysAnchored(t *testing.T) {
	t.Parallel()
	target := writeConfig(t, `
functions:
  "internal/fixture/loader.go:*":
    defaults:
      dir: "/fixtures/sample"
`)
	file, err := config.Load(target)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	for _, relpath := range []string{
		"loader.go",
		"internal/other/loader.go",
	} {
		if entry := file.MatchTarget(config.TargetRelpath(relpath), "loadOne"); len(entry.Defaults) != 0 {
			t.Errorf("relpath %q unexpectedly matched path-scoped glob (defaults=%+v)", relpath, entry.Defaults)
		}
	}
	if entry := file.MatchTarget(config.TargetRelpath("internal/fixture/loader.go"), "loadOne"); len(entry.Defaults) != 1 {
		t.Errorf("path-scoped glob failed to match its own path: %+v", entry.Defaults)
	}
}

// An anchored pattern is more specific than any filename-scoped one that also
// matches, so basename-fallback matches must never outrank — or tie — anchored
// ones. The tie case matters most: two matches with equal scores are separated
// by Go's randomized map iteration, which would make the same file resolve to
// different config on different requests within one process. Each case is run
// repeatedly for that reason; a single call can pass on luck.
func TestMatchTarget_AnchoredBeatsBasenameFallback(t *testing.T) {
	t.Parallel()
	cases := []struct {
		name     string
		competes string // the anchored key competing with the "loader.go" filename key
		want     string
	}{
		// Exact basename vs exact full path: both score 2000 without tiering.
		{"full path literal", `"internal/fixture/loader.go:loadOne"`, "anchored"},
		// Exact basename (1000) vs directory-scoped glob (len ~= 20): the
		// fallback would win on score alone despite being far less specific.
		{"directory-scoped glob", `"internal/fixture/*.go:loadOne"`, "anchored"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()
			target := writeConfig(t, `
functions:
  "loader.go:loadOne":
    defaults:
      dir: "fallback"
  `+tc.competes+`:
    defaults:
      dir: "anchored"
`)
			file, err := config.Load(target)
			if err != nil {
				t.Fatalf("Load: %v", err)
			}
			for i := range 200 {
				entry := file.MatchTarget(config.TargetRelpath("internal/fixture/loader.go"), "loadOne")
				if !bytes.Equal(entry.Defaults["dir"].JSON, []byte(`"`+tc.want+`"`)) {
					t.Fatalf("iteration %d: dir default = %s, want %q", i, string(entry.Defaults["dir"].JSON), tc.want)
				}
			}
		})
	}
}

// A full-path pattern is more specific than a filename glob that also matches.
func TestMatchTarget_FullPathBeatsFilenameGlob(t *testing.T) {
	t.Parallel()
	target := writeConfig(t, `
functions:
  "*.resolvers.go:*":
    defaults:
      dir: "glob"
  "api/graph/resolver/auth.resolvers.go:*":
    defaults:
      dir: "anchored"
`)
	file, err := config.Load(target)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	entry := file.MatchTarget(config.TargetRelpath("api/graph/resolver/auth.resolvers.go"), "CreateTeam")
	if !bytes.Equal(entry.Defaults["dir"].JSON, []byte(`"anchored"`)) {
		t.Errorf("dir default = %s, want \"anchored\"", string(entry.Defaults["dir"].JSON))
	}
}

// MatchTargetAnchored is the fail-closed lookup used by the safety-policy gate:
// hint-key breadth must not widen which files a `policy.allow` block covers.
func TestMatchTargetAnchored_NoBasenameFallback(t *testing.T) {
	t.Parallel()
	target := writeConfig(t, `
functions:
  "main.go:*":
    policy:
      allow: [network]
`)
	file, err := config.Load(target)
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	// A nested main.go is a different file; the operator scoped the root one.
	if entry := file.MatchTargetAnchored(config.TargetRelpath("cmd/api/main.go"), "main"); entry.Policy != nil {
		t.Errorf("anchored lookup granted policy to cmd/api/main.go: %+v", entry.Policy)
	}
	// The hint lookup deliberately does apply to it.
	if entry := file.MatchTarget(config.TargetRelpath("cmd/api/main.go"), "main"); entry.Policy == nil {
		t.Error("hint lookup should match the filename-scoped key")
	}
	// The file the key names still matches under both.
	if entry := file.MatchTargetAnchored(config.TargetRelpath("main.go"), "main"); entry.Policy == nil {
		t.Error("anchored lookup failed to match the root main.go")
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
