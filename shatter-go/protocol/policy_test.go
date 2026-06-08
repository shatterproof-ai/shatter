package protocol

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/config"
)

// --- Unit tests for classifyFunction / evaluatePolicy / buildAllowedSet ---

func TestClassifyFunction_DatabaseParamIsClassified(t *testing.T) {
	fa := &FunctionAnalysis{
		Name: "UsesDB",
		Params: []ParamInfo{
			{Name: "db", Type: TypeInfo{Kind: "opaque", Label: "sql.DB"}, TypeName: ptr("*sql.DB")},
		},
	}
	uses := classifyFunction(fa)
	if len(uses) != 1 {
		t.Fatalf("expected 1 classified use, got %d: %+v", len(uses), uses)
	}
	if uses[0].Class != ClassDatabase {
		t.Errorf("class = %q, want %q", uses[0].Class, ClassDatabase)
	}
	if !strings.Contains(uses[0].Component, "sql.DB") {
		t.Errorf("component = %q, want to contain sql.DB", uses[0].Component)
	}
}

func TestClassifyFunction_SafeHTTPRuntimeValueParamsAreNotNetwork(t *testing.T) {
	fa := &FunctionAnalysis{
		Name: "UsesInMemoryHTTPValues",
		Params: []ParamInfo{
			{Name: "w", Type: TypeInfo{Kind: "unknown", Label: "http.ResponseWriter"}, TypeName: ptr("http.ResponseWriter")},
			{Name: "r", Type: TypeInfo{Kind: "unknown", Label: "http.Request"}, TypeName: ptr("*http.Request")},
		},
	}
	if uses := classifyFunction(fa); len(uses) != 0 {
		t.Fatalf("safe httptest-backed runtime values should not be policy-denied, got %+v", uses)
	}
}

func TestClassifyFunction_HTTPClientParamStillNetwork(t *testing.T) {
	fa := &FunctionAnalysis{
		Name: "UsesHTTPClient",
		Params: []ParamInfo{
			{Name: "client", Type: TypeInfo{Kind: "opaque", Label: "http.Client"}, TypeName: ptr("*http.Client")},
		},
	}
	uses := classifyFunction(fa)
	if len(uses) != 1 || uses[0].Class != ClassNetwork {
		t.Fatalf("expected http.Client to remain network-classified, got %+v", uses)
	}
}

func TestClassifyFunction_SubprocessDependencyIsClassified(t *testing.T) {
	fa := &FunctionAnalysis{
		Name: "Runs",
		Dependencies: []ExternalDependency{
			{Symbol: "exec.Command", SourceModule: "os/exec", Kind: "call"},
		},
	}
	uses := classifyFunction(fa)
	if len(uses) != 1 || uses[0].Class != ClassSubprocess {
		t.Fatalf("expected subprocess classification, got %+v", uses)
	}
}

func TestClassifyFunction_BrowserAutomationDependencyIsClassified(t *testing.T) {
	fa := &FunctionAnalysis{
		Name: "LaunchesBrowser",
		Dependencies: []ExternalDependency{
			{Symbol: "New", SourceModule: "github.com/go-rod/rod/lib/launcher", Kind: "call"},
		},
	}
	uses := classifyFunction(fa)
	if len(uses) != 1 || uses[0].Class != ClassSubprocess {
		t.Fatalf("expected browser launcher to classify as subprocess, got %+v", uses)
	}
}

func TestClassifyFunction_BrowserReturnTypeIsClassified(t *testing.T) {
	fa := &FunctionAnalysis{
		Name:       "NewBrowser",
		ReturnType: TypeInfo{Kind: "opaque", Label: "scraper.Browser"},
	}
	uses := classifyFunction(fa)
	if len(uses) != 1 || uses[0].Class != ClassSubprocess {
		t.Fatalf("expected browser return type to classify as subprocess, got %+v", uses)
	}
}

func TestClassifyFunction_LocalBrowserConstructorReturnIsClassified(t *testing.T) {
	fa := &FunctionAnalysis{
		Name: "New",
		ReturnType: TypeInfo{Kind: "object", Fields: []ObjectField{
			{
				Name: "_0",
				Type: TypeInfo{
					Kind:  "nullable",
					Inner: &TypeInfo{Kind: "object", Label: "Browser"},
				},
			},
			{Name: "_1", Type: TypeInfo{Kind: "complex", ComplexKind: "error"}},
		}},
	}
	uses := classifyFunction(fa)
	if len(uses) != 1 || uses[0].Class != ClassSubprocess {
		t.Fatalf("expected local browser constructor return to classify as subprocess, got %+v", uses)
	}
}

func TestClassifyFunction_LocalBrowserHelperIsClassified(t *testing.T) {
	root := &FunctionAnalysis{
		Name: "Scrape",
		Dependencies: []ExternalDependency{
			{Symbol: "newBrowser", SourceModule: "example.com/project", Kind: "call"},
		},
	}
	helper := &FunctionAnalysis{
		Name:       "newBrowser",
		ReturnType: TypeInfo{Kind: "nullable", Inner: &TypeInfo{Kind: "opaque", Label: "scraper.Browser"}},
	}
	uses := classifyFunctionWithLocalDependencies(root, func(symbol string) *FunctionAnalysis {
		if symbol == "newBrowser" {
			return helper
		}
		return nil
	})
	if len(uses) != 1 || uses[0].Class != ClassSubprocess {
		t.Fatalf("expected local browser helper to classify as subprocess, got %+v", uses)
	}
}

func TestClassifyFunction_PureFunctionHasNoUses(t *testing.T) {
	fa := &FunctionAnalysis{
		Name: "Add",
		Params: []ParamInfo{
			{Name: "a", Type: TypeInfo{Kind: "int"}},
			{Name: "b", Type: TypeInfo{Kind: "int"}},
		},
	}
	if uses := classifyFunction(fa); len(uses) != 0 {
		t.Errorf("expected no classified uses, got %+v", uses)
	}
}

func TestClassifyFunction_QualifiedOSLocalFSSymbolsAreAllowed(t *testing.T) {
	for _, symbol := range []string{"os.ReadFile", "os.Stat", "os.ReadDir"} {
		t.Run(symbol, func(t *testing.T) {
			fa := &FunctionAnalysis{
				Name: "UsesLocalFS",
				Dependencies: []ExternalDependency{
					{Symbol: symbol, SourceModule: "os", Kind: "call"},
				},
			}
			uses := classifyFunction(fa)
			if len(uses) != 1 || uses[0].Class != ClassLocalFS {
				t.Fatalf("expected %s to classify as local_fs, got %+v", symbol, uses)
			}
		})
	}
}

func TestClassifyFunction_QualifiedOSProcessGlobalStillDenied(t *testing.T) {
	fa := &FunctionAnalysis{
		Name: "MutatesProcess",
		Dependencies: []ExternalDependency{
			{Symbol: "os.Setenv", SourceModule: "os", Kind: "call"},
		},
	}
	uses := classifyFunction(fa)
	if len(uses) != 1 || uses[0].Class != ClassProcessGlobal {
		t.Fatalf("expected os.Setenv to classify as process_global, got %+v", uses)
	}
}

func TestClassifyFunction_QualifiedOSPurePredicateHasNoUses(t *testing.T) {
	fa := &FunctionAnalysis{
		Name: "ChecksError",
		Dependencies: []ExternalDependency{
			{Symbol: "os.IsNotExist", SourceModule: "os", Kind: "call"},
		},
	}
	if uses := classifyFunction(fa); len(uses) != 0 {
		t.Fatalf("expected os.IsNotExist to be treated as pure, got %+v", uses)
	}
}

func TestClassifyFunction_UnrecognizedModuleIsUnknownHigh(t *testing.T) {
	fa := &FunctionAnalysis{
		Dependencies: []ExternalDependency{
			{Symbol: "Read", SourceModule: "crypto/rand"},
		},
	}
	uses := classifyFunction(fa)
	if len(uses) != 1 || uses[0].Class != ClassUnknownHigh {
		t.Fatalf("expected unknown_high for crypto/rand, got %+v", uses)
	}
}

func TestEvaluatePolicy_DefaultDeniesDatabase(t *testing.T) {
	uses := []ClassifiedUse{
		{Class: ClassDatabase, Component: "*sql.DB", Evidence: "param db"},
	}
	decision := evaluatePolicy(uses, defaultAllowedClasses())
	if decision.Allow {
		t.Fatal("expected deny, got allow")
	}
	if decision.Offending.Class != ClassDatabase {
		t.Errorf("offending class = %q, want %q", decision.Offending.Class, ClassDatabase)
	}
	if !strings.Contains(decision.Reason, "database") {
		t.Errorf("reason %q does not mention database", decision.Reason)
	}
}

func TestEvaluatePolicy_PureIsAllowed(t *testing.T) {
	decision := evaluatePolicy(nil, defaultAllowedClasses())
	if !decision.Allow {
		t.Fatalf("expected allow for pure function, got %+v", decision)
	}
}

func TestBuildAllowedSet_UnknownEntryIsLoggedAndIgnored(t *testing.T) {
	var captured []string
	allowed := buildAllowedSet([]string{"gibberish", "database"}, func(raw string) {
		captured = append(captured, raw)
	})
	if !allowed[ClassDatabase] {
		t.Error("database should be allowed after override")
	}
	if allowed["gibberish"] {
		t.Error("gibberish should not be an allowed class")
	}
	if len(captured) != 1 || captured[0] != "gibberish" {
		t.Errorf("expected warn for gibberish only, got %+v", captured)
	}
}

// --- Config loader tests ---

func TestConfigLoad_MissingFileReturnsZero(t *testing.T) {
	tmp := t.TempDir()
	dummy := filepath.Join(tmp, "x.go")
	if err := os.WriteFile(dummy, []byte("package x"), 0o644); err != nil {
		t.Fatal(err)
	}
	file, err := config.Load(dummy)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if len(file.Functions) != 0 {
		t.Errorf("expected empty File, got %+v", file)
	}
}

func TestConfigLoad_MatchTargetHonoursSpecificity(t *testing.T) {
	file := config.File{
		Functions: map[string]config.FunctionConfig{
			"*:*":            {Policy: &config.PolicyConfig{Allow: []string{"network"}}},
			"user.go:UsesDB": {Policy: &config.PolicyConfig{Allow: []string{"database"}}},
			"*_test.go:*":    {Policy: &config.PolicyConfig{Allow: []string{"subprocess"}}},
		},
	}
	entry := file.MatchTarget("user.go", "UsesDB")
	if entry.Policy == nil || len(entry.Policy.Allow) == 0 || entry.Policy.Allow[0] != "database" {
		t.Errorf("expected database-specific match, got %+v", entry)
	}
}

func TestConfigLoad_ParsesPolicySection(t *testing.T) {
	tmp := t.TempDir()
	shatterDir := filepath.Join(tmp, ".shatter")
	if err := os.MkdirAll(shatterDir, 0o755); err != nil {
		t.Fatal(err)
	}
	yaml := []byte(`functions:
  "policy_target.go:UsesDB":
    policy:
      allow: [database]
`)
	if err := os.WriteFile(filepath.Join(shatterDir, "config.yaml"), yaml, 0o644); err != nil {
		t.Fatal(err)
	}
	target := filepath.Join(tmp, "policy_target.go")
	if err := os.WriteFile(target, []byte("package x"), 0o644); err != nil {
		t.Fatal(err)
	}
	f, err := config.Load(target)
	if err != nil {
		t.Fatalf("load: %v", err)
	}
	entry := f.MatchTarget("policy_target.go", "UsesDB")
	if entry.Policy == nil || len(entry.Policy.Allow) != 1 || entry.Policy.Allow[0] != "database" {
		t.Errorf("expected database in allow list, got %+v", entry.Policy)
	}
}

// --- Acceptance scenarios (end-to-end through the handler) ---

// runExecuteWithLoader drives analyze+execute against a real handler
// instance with an injected policy config loader, and returns the
// execute response.
func runExecuteWithLoader(t *testing.T, file, function string, loader func(string) (config.File, error)) Response {
	t.Helper()
	requests := []string{
		reqJSON(1, "handshake", `"capabilities":["analyze","execute"]`),
		reqJSON(2, "analyze", fmt.Sprintf(`"file":%q,"functions":[%q]`, file, function)),
		reqJSON(3, "execute", fmt.Sprintf(`"file":%q,"function":%q,"inputs":[null]`, file, function)),
		reqJSON(4, "shutdown"),
	}
	input := strings.NewReader(strings.Join(requests, "\n") + "\n")
	var output strings.Builder
	handler := NewHandler(input, &output, os.NewFile(0, os.DevNull))
	handler.policyConfigLoader = loader
	if err := handler.Run(); err != nil {
		t.Fatalf("handler.Run: %v", err)
	}
	lines := strings.Split(strings.TrimSpace(output.String()), "\n")
	// Execute response is the third non-empty line (index 2).
	var executeLine string
	nonEmpty := 0
	for _, l := range lines {
		if strings.TrimSpace(l) == "" {
			continue
		}
		if nonEmpty == 2 {
			executeLine = l
			break
		}
		nonEmpty++
	}
	if executeLine == "" {
		t.Fatalf("no execute response in output:\n%s", output.String())
	}
	var resp Response
	if err := json.Unmarshal([]byte(executeLine), &resp); err != nil {
		t.Fatalf("unmarshal execute response: %v (raw: %s)", err, executeLine)
	}
	return resp
}

// TestExecute_DefaultPolicy_SkipsDatabaseTarget is the first acceptance
// scenario: a target accepting *sql.DB with no .shatter/config.yaml
// produces outcome.status == skipped_by_policy, with a reason that
// mentions the database class.
func TestExecute_DefaultPolicy_SkipsDatabaseTarget(t *testing.T) {
	resp := runExecuteWithLoader(t, "testdata/opaque.go", "AcceptsSqlDB", func(string) (config.File, error) {
		return config.File{}, nil
	})
	if resp.Outcome == nil {
		t.Fatalf("expected Outcome on response, got: %+v", resp)
	}
	if resp.Outcome.Status != OutcomeStatusSkippedByPolicy {
		t.Errorf("outcome.status = %q, want %q (full resp: %+v)", resp.Outcome.Status, OutcomeStatusSkippedByPolicy, resp)
	}
	if resp.Outcome.ShortReason == nil || !strings.Contains(*resp.Outcome.ShortReason, "database") {
		t.Errorf("reason should mention database class, got: %v", resp.Outcome.ShortReason)
	}
	if resp.Outcome.ShortReason == nil || !strings.Contains(*resp.Outcome.ShortReason, "sql.DB") {
		t.Errorf("reason should mention sql.DB component, got: %v", resp.Outcome.ShortReason)
	}
	// str-adfp: Performance must be present so the core can deserialize the response.
	if resp.Performance == nil {
		t.Fatalf("performance field must be present on skipped_by_policy responses")
	}
}

func TestExecute_DefaultPolicy_AllowsResponseWriterRuntimeValueTarget(t *testing.T) {
	resp := runExecuteWithLoader(t, "testdata/opaque.go", "AcceptsResponseWriter", func(string) (config.File, error) {
		return config.File{}, nil
	})
	if resp.Outcome != nil && resp.Outcome.Status == OutcomeStatusSkippedByPolicy {
		t.Fatalf("http.ResponseWriter should reach runtime-value planning/execution, got skipped_by_policy: %v", resp.Outcome.ShortReason)
	}
}

func TestPrepare_DefaultPolicy_DoesNotBuildBrowserTarget(t *testing.T) {
	file := "testdata/opaque.go"
	function := "LaunchesBrowser"
	handler := NewHandler(strings.NewReader(""), io.Discard, io.Discard)
	handler.policyConfigLoader = func(string) (config.File, error) {
		return config.File{}, nil
	}
	handler.cachedAnalyses[file+"\x00"+function] = &FunctionAnalysis{
		Name: function,
		Dependencies: []ExternalDependency{
			{Symbol: "NewContext", SourceModule: "example.com/local", Kind: "call"},
		},
	}
	handler.cachedAnalyses[file+"\x00"+"NewContext"] = &FunctionAnalysis{
		Name: "NewContext",
		Dependencies: []ExternalDependency{
			{Symbol: "New", SourceModule: "github.com/go-rod/rod/lib/launcher", Kind: "call"},
		},
	}

	resp := handler.handlePrepare(Response{ProtocolVersion: ProtocolVersion, ID: 1}, Request{
		ProtocolVersion: ProtocolVersion,
		ID:              1,
		Command:         "prepare",
		File:            file,
		Function:        &function,
	})

	if resp.Status != "prepare" {
		t.Fatalf("prepare status = %q, want prepare (full resp: %+v)", resp.Status, resp)
	}
	if resp.PrepareID == "" {
		t.Fatalf("prepare_id must be populated for policy-skipped prepare")
	}
	if len(handler.preparedHarnesses) != 0 {
		t.Fatalf("policy-skipped prepare must not build harnesses, got %d", len(handler.preparedHarnesses))
	}
}

func TestPrepare_DefaultPolicy_AnalyzesMissingPolicyCache(t *testing.T) {
	file := "testdata/opaque.go"
	function := "AcceptsSqlDB"
	handler := NewHandler(strings.NewReader(""), io.Discard, io.Discard)
	handler.policyConfigLoader = func(string) (config.File, error) {
		return config.File{}, nil
	}

	resp := handler.handlePrepare(Response{ProtocolVersion: ProtocolVersion, ID: 1}, Request{
		ProtocolVersion: ProtocolVersion,
		ID:              1,
		Command:         "prepare",
		File:            file,
		Function:        &function,
	})

	if resp.Status != "prepare" {
		t.Fatalf("prepare status = %q, want prepare (full resp: %+v)", resp.Status, resp)
	}
	if resp.PrepareID == "" {
		t.Fatalf("prepare_id must be populated for policy-skipped prepare")
	}
	if len(handler.preparedHarnesses) != 0 {
		t.Fatalf("policy-skipped prepare must not build harnesses, got %d", len(handler.preparedHarnesses))
	}
	if handler.cachedAnalyses[file+"\x00"+function] == nil {
		t.Fatalf("expected prepare to populate policy analysis cache for %s", function)
	}
}

// TestExecute_PolicyAllowOverride_RunsDatabaseTarget is the second
// acceptance scenario: with policy.allow=[database] for the target, the
// policy gate does not short-circuit — execution proceeds past the gate
// (evidenced by an outcome other than skipped_by_policy).
func TestExecute_PolicyAllowOverride_RunsDatabaseTarget(t *testing.T) {
	override := config.File{
		Functions: map[string]config.FunctionConfig{
			"opaque.go:AcceptsSqlDB":          {Policy: &config.PolicyConfig{Allow: []string{"database"}}},
			"testdata/opaque.go:AcceptsSqlDB": {Policy: &config.PolicyConfig{Allow: []string{"database"}}},
		},
	}
	resp := runExecuteWithLoader(t, "testdata/opaque.go", "AcceptsSqlDB", func(string) (config.File, error) {
		return override, nil
	})
	if resp.Outcome != nil && resp.Outcome.Status == OutcomeStatusSkippedByPolicy {
		t.Fatalf("policy gate should have allowed execution; got skipped_by_policy: %v", resp.Outcome.ShortReason)
	}
}

func ptr[T any](v T) *T { return &v }
