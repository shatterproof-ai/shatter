package instrument

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"time"
)

func writeExecTestSource(t *testing.T, dir, filename, content string) string {
	t.Helper()
	path := filepath.Join(dir, filename)
	if err := os.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatalf("writing test source: %v", err)
	}
	return path
}

func TestExecuteFunctionReturnsIntResult(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func add(a int, b int) int {
	return a + b
}
`)
	result, err := ExecuteFunction(src, "add", []json.RawMessage{
		json.RawMessage("3"),
		json.RawMessage("4"),
	})
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}
	if result.ThrownError != nil {
		t.Fatalf("unexpected error: %+v", result.ThrownError)
	}

	var retVal int
	if err := json.Unmarshal(result.ReturnValue, &retVal); err != nil {
		t.Fatalf("parsing return value: %v (raw: %s)", err, string(result.ReturnValue))
	}
	if retVal != 7 {
		t.Errorf("expected return value 7, got %d", retVal)
	}
}

func TestExecuteFunctionReturnsStringResult(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func greet(name string) string {
	return "hello " + name
}
`)
	result, err := ExecuteFunction(src, "greet", []json.RawMessage{
		json.RawMessage(`"world"`),
	})
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}
	if result.ThrownError != nil {
		t.Fatalf("unexpected error: %+v", result.ThrownError)
	}

	var retVal string
	if err := json.Unmarshal(result.ReturnValue, &retVal); err != nil {
		t.Fatalf("parsing return value: %v", err)
	}
	if retVal != "hello world" {
		t.Errorf("expected %q, got %q", "hello world", retVal)
	}
}

func TestExecuteFunctionRecordsBranches(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func classify(x int) string {
	if x > 0 {
		return "positive"
	}
	return "nonpositive"
}
`)
	result, err := ExecuteFunction(src, "classify", []json.RawMessage{
		json.RawMessage("5"),
	})
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}
	if result.ThrownError != nil {
		t.Fatalf("unexpected error: %+v", result.ThrownError)
	}

	if len(result.BranchPath) == 0 {
		t.Fatal("expected branch decisions to be recorded")
	}

	// With x=5, branch 0 (x > 0) should be taken=true
	found := false
	for _, b := range result.BranchPath {
		if b.BranchID == 0 && b.Taken {
			found = true
			break
		}
	}
	if !found {
		t.Errorf("expected branch 0 taken=true, got: %+v", result.BranchPath)
	}

	// Should have recorded lines
	if len(result.LinesExecuted) == 0 {
		t.Error("expected lines to be recorded")
	}

	// Return value should be "positive"
	var retVal string
	if err := json.Unmarshal(result.ReturnValue, &retVal); err != nil {
		t.Fatalf("parsing return value: %v", err)
	}
	if retVal != "positive" {
		t.Errorf("expected %q, got %q", "positive", retVal)
	}
}

func TestExecuteFunctionRecordsNegativeBranch(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func classify(x int) string {
	if x > 0 {
		return "positive"
	}
	return "nonpositive"
}
`)
	result, err := ExecuteFunction(src, "classify", []json.RawMessage{
		json.RawMessage("-1"),
	})
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}

	// Branch 0 should be taken=false
	found := false
	for _, b := range result.BranchPath {
		if b.BranchID == 0 && !b.Taken {
			found = true
			break
		}
	}
	if !found {
		t.Errorf("expected branch 0 taken=false, got: %+v", result.BranchPath)
	}

	var retVal string
	if err := json.Unmarshal(result.ReturnValue, &retVal); err != nil {
		t.Fatalf("parsing return value: %v", err)
	}
	if retVal != "nonpositive" {
		t.Errorf("expected %q, got %q", "nonpositive", retVal)
	}
}

func TestExecuteFunctionErrorsOnWrongArgCount(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func add(a int, b int) int {
	return a + b
}
`)
	_, err := ExecuteFunction(src, "add", []json.RawMessage{
		json.RawMessage("3"),
	})
	if err == nil {
		t.Error("expected error for wrong argument count")
	}
}

func TestExecuteFunctionErrorsOnMissingFunction(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func add(a int, b int) int {
	return a + b
}
`)
	_, err := ExecuteFunction(src, "nonexistent", nil)
	if err == nil {
		t.Error("expected error for missing function")
	}
}

func TestExecuteFunctionHandlesNoReturnValue(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

import "fmt"

func sayHello(name string) {
	fmt.Println("hello", name)
}
`)
	result, err := ExecuteFunction(src, "sayHello", []json.RawMessage{
		json.RawMessage(`"world"`),
	})
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}
	if result.ThrownError != nil {
		t.Fatalf("unexpected error: %+v", result.ThrownError)
	}

	// No return value expected
	if result.ReturnValue != nil {
		t.Errorf("expected nil return value, got: %s", string(result.ReturnValue))
	}
}

func TestExecuteFunctionMeasuresPerformance(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func identity(x int) int {
	return x
}
`)
	result, err := ExecuteFunction(src, "identity", []json.RawMessage{
		json.RawMessage("42"),
	})
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}

	// WallTimeMs should be positive
	if result.Performance.WallTimeMs <= 0 {
		t.Errorf("expected positive wall time, got %f", result.Performance.WallTimeMs)
	}

	// CPUTimeUs should be non-negative (may be 0 for very fast functions)
	if result.Performance.CPUTimeUs < 0 {
		t.Errorf("expected non-negative CPU time, got %d", result.Performance.CPUTimeUs)
	}

	// HeapAllocatedBytes should be non-negative
	if result.Performance.HeapAllocatedBytes < 0 {
		t.Errorf("expected non-negative heap allocated, got %d", result.Performance.HeapAllocatedBytes)
	}
}

func TestExecuteFunctionHandlesPanic(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func boom(x int) int {
	panic("kaboom")
}
`)
	result, err := ExecuteFunction(src, "boom", []json.RawMessage{
		json.RawMessage("1"),
	})
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}

	if result.ThrownError == nil {
		t.Fatal("expected error for panicking function")
	}
	if result.ThrownError.ErrorType != "runtime_error" {
		t.Errorf("expected error_type runtime_error, got %q", result.ThrownError.ErrorType)
	}
}

func TestExecuteFunctionWithBoolInput(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func negate(b bool) bool {
	return !b
}
`)
	result, err := ExecuteFunction(src, "negate", []json.RawMessage{
		json.RawMessage("true"),
	})
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}
	if result.ThrownError != nil {
		t.Fatalf("unexpected error: %+v", result.ThrownError)
	}

	var retVal bool
	if err := json.Unmarshal(result.ReturnValue, &retVal); err != nil {
		t.Fatalf("parsing return value: %v", err)
	}
	if retVal != false {
		t.Errorf("expected false, got %v", retVal)
	}
}

func TestExecuteFunctionWithFloat64Input(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func double(x float64) float64 {
	return x * 2
}
`)
	result, err := ExecuteFunction(src, "double", []json.RawMessage{
		json.RawMessage("3.5"),
	})
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}
	if result.ThrownError != nil {
		t.Fatalf("unexpected error: %+v", result.ThrownError)
	}

	var retVal float64
	if err := json.Unmarshal(result.ReturnValue, &retVal); err != nil {
		t.Fatalf("parsing return value: %v", err)
	}
	if retVal != 7.0 {
		t.Errorf("expected 7.0, got %f", retVal)
	}
}

func TestExecuteFunctionWithBuildFailure(t *testing.T) {
	srcDir := t.TempDir()
	// Write invalid Go code that will fail to compile
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func broken(x int) int {
	return x +
}
`)
	_, err := ExecuteFunction(src, "broken", []json.RawMessage{
		json.RawMessage("1"),
	})
	// Should get an error (either parse or build failure)
	if err == nil {
		t.Error("expected error for code with syntax error")
	}
}

func TestExecuteFunctionWithMultipleBranches(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func multiCheck(x int, y int) string {
	if x > 0 {
		if y > 0 {
			return "both positive"
		}
		return "x positive only"
	}
	return "x not positive"
}
`)
	result, err := ExecuteFunction(src, "multiCheck", []json.RawMessage{
		json.RawMessage("5"),
		json.RawMessage("10"),
	})
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}
	if result.ThrownError != nil {
		t.Fatalf("unexpected error: %+v", result.ThrownError)
	}

	// Should have at least 2 branch decisions (x > 0 and y > 0)
	if len(result.BranchPath) < 2 {
		t.Errorf("expected at least 2 branch decisions, got %d: %+v", len(result.BranchPath), result.BranchPath)
	}

	var retVal string
	if err := json.Unmarshal(result.ReturnValue, &retVal); err != nil {
		t.Fatalf("parsing return value: %v", err)
	}
	if retVal != "both positive" {
		t.Errorf("expected %q, got %q", "both positive", retVal)
	}
}

func TestExecTimeoutDefaultIs5s(t *testing.T) {
	os.Unsetenv("SHATTER_EXEC_TIMEOUT")
	if d := execTimeout(); d != defaultExecTimeout {
		t.Errorf("expected %v default, got %v", defaultExecTimeout, d)
	}
}

func TestExecTimeoutReadsEnvVar(t *testing.T) {
	t.Setenv("SHATTER_EXEC_TIMEOUT", "25")
	if d := execTimeout(); d != 25*time.Second {
		t.Errorf("expected 25s, got %v", d)
	}
}

func TestExecTimeoutIgnoresInvalidEnvVar(t *testing.T) {
	t.Setenv("SHATTER_EXEC_TIMEOUT", "not-a-number")
	if d := execTimeout(); d != defaultExecTimeout {
		t.Errorf("expected %v fallback, got %v", defaultExecTimeout, d)
	}
}

func TestExecTimeoutIgnoresZero(t *testing.T) {
	t.Setenv("SHATTER_EXEC_TIMEOUT", "0")
	if d := execTimeout(); d != defaultExecTimeout {
		t.Errorf("expected %v fallback for zero, got %v", defaultExecTimeout, d)
	}
}

func TestExecTimeoutIgnoresNegative(t *testing.T) {
	t.Setenv("SHATTER_EXEC_TIMEOUT", "-5")
	if d := execTimeout(); d != defaultExecTimeout {
		t.Errorf("expected %v fallback for negative, got %v", defaultExecTimeout, d)
	}
}

func TestSanitizeMockName(t *testing.T) {
	tests := []struct {
		input string
		want  string
	}{
		{"readFile", "readFile"},
		{"fs.readFile", "fs_readFile"},
		{"@prisma/client.findMany", "_prisma_client_findMany"},
		{"http:get", "http_get"},
	}
	for _, tc := range tests {
		got := sanitizeMockName(tc.input)
		if got != tc.want {
			t.Errorf("sanitizeMockName(%q) = %q, want %q", tc.input, got, tc.want)
		}
	}
}

func TestGenerateMockFileContainsMockFunctions(t *testing.T) {
	mocks := []MockConfig{
		{
			Symbol:           "fs.readFile",
			ReturnValues:     []any{"file contents"},
			ShouldTrackCalls: true,
			DefaultBehavior:  "repeat_last",
		},
		{
			Symbol:           "lodash.map",
			ReturnValues:     nil,
			ShouldTrackCalls: false,
			DefaultBehavior:  "passthrough",
		},
	}

	source := generateMockFile(mocks, "/tmp/calls.json")

	// Should contain a mock function for fs.readFile
	if !contains(source, "ShatterMock_fs_readFile") {
		t.Error("expected ShatterMock_fs_readFile in generated source")
	}

	// Should NOT contain a mock function for passthrough mocks
	if contains(source, "ShatterMock_lodash_map") {
		t.Error("passthrough mock should not generate a function")
	}

	// Should contain call tracking for fs.readFile
	if !contains(source, `shatterRecordMockCall("fs.readFile"`) {
		t.Error("expected call tracking for fs.readFile")
	}

	// Should contain the dump function
	if !contains(source, "shatterDumpMockCalls") {
		t.Error("expected shatterDumpMockCalls function")
	}
}

func contains(s, substr string) bool {
	return len(s) > 0 && len(substr) > 0 && // prevent trivial matches
		len(s) >= len(substr) &&
		indexOf(s, substr) >= 0
}

func indexOf(s, substr string) int {
	for i := 0; i <= len(s)-len(substr); i++ {
		if s[i:i+len(substr)] == substr {
			return i
		}
	}
	return -1
}

func TestFlattenMocksEmpty(t *testing.T) {
	result := flattenMocks(nil)
	if result != nil {
		t.Errorf("expected nil for empty mocks, got %v", result)
	}

	result = flattenMocks([][]MockConfig{{}})
	if result != nil {
		t.Errorf("expected nil for empty inner slice, got %v", result)
	}
}

func TestFlattenMocksReturnsFirst(t *testing.T) {
	mocks := [][]MockConfig{{
		{Symbol: "test", ReturnValues: []any{1}},
	}}
	result := flattenMocks(mocks)
	if len(result) != 1 || result[0].Symbol != "test" {
		t.Errorf("expected single mock, got %v", result)
	}
}

func TestBuildTimeoutDefaultIs30s(t *testing.T) {
	os.Unsetenv("SHATTER_BUILD_TIMEOUT")
	if d := buildTimeout(); d != defaultBuildTimeout {
		t.Errorf("expected 30s default, got %v", d)
	}
}

func TestBuildTimeoutReadsEnvVar(t *testing.T) {
	t.Setenv("SHATTER_BUILD_TIMEOUT", "60")
	if d := buildTimeout(); d != 60*time.Second {
		t.Errorf("expected 60s, got %v", d)
	}
}

func TestStripLocalPkg(t *testing.T) {
	tests := []struct {
		typeStr, pkg, want string
	}{
		{"examples.User", "examples", "User"},
		{"[]examples.User", "examples", "[]User"},
		{"map[string]examples.User", "examples", "map[string]User"},
		{"int", "examples", "int"},
		{"other.Type", "examples", "other.Type"},
		{"examples.User", "", "examples.User"},
	}
	for _, tt := range tests {
		got := stripLocalPkg(tt.typeStr, tt.pkg)
		if got != tt.want {
			t.Errorf("stripLocalPkg(%q, %q) = %q, want %q", tt.typeStr, tt.pkg, got, tt.want)
		}
	}
}

// TestExecuteFunctionWithStructParam verifies that functions with
// package-local struct parameters work after package-to-main rewriting.
func TestExecuteFunctionWithStructParam(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package mylib

type Point struct {
	X int
	Y int
}

func SumPoint(p Point) int {
	return p.X + p.Y
}
`)
	result, err := ExecuteFunction(src, "SumPoint", []json.RawMessage{
		json.RawMessage(`{"X": 3, "Y": 7}`),
	})
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}
	var retVal int
	if err := json.Unmarshal(result.ReturnValue, &retVal); err != nil {
		t.Fatalf("unmarshal return: %v", err)
	}
	if retVal != 10 {
		t.Errorf("expected 10, got %d", retVal)
	}
}

func TestGenerateMockFileThrowError(t *testing.T) {
	mocks := []MockConfig{
		{
			Symbol:           "db.query",
			ReturnValues:     []any{map[string]any{"message": "connection refused"}},
			ShouldTrackCalls: true,
			DefaultBehavior:  "throw_error",
		},
	}

	source := generateMockFile(mocks, "/tmp/calls.json")

	if !contains(source, "ShatterMock_db_query") {
		t.Error("expected ShatterMock_db_query in generated source")
	}

	if !contains(source, "panic(msg)") {
		t.Error("expected panic(msg) in throw_error mock")
	}

	if !contains(source, `shatterRecordMockCall("db.query"`) {
		t.Error("expected call tracking before panic")
	}

	// Should not contain "return retVal" for throw_error mocks
	if contains(source, "return retVal") {
		t.Error("throw_error mock should not return a value")
	}
}

func TestGenerateMockFileThrowErrorNoTrackCalls(t *testing.T) {
	mocks := []MockConfig{
		{
			Symbol:          "net.dial",
			ReturnValues:    []any{map[string]any{"message": "timeout"}},
			DefaultBehavior: "throw_error",
		},
	}

	source := generateMockFile(mocks, "/tmp/calls.json")

	if !contains(source, "panic(msg)") {
		t.Error("expected panic in throw_error mock")
	}

	// Should NOT contain call tracking when ShouldTrackCalls is false
	if contains(source, `shatterRecordMockCall("net.dial"`) {
		t.Error("should not track calls when ShouldTrackCalls is false")
	}
}

func TestDiscoverDependenciesFindsThirdParty(t *testing.T) {
	srcDir := t.TempDir()
	src := filepath.Join(srcDir, "target.go")
	if err := os.WriteFile(src, []byte(`package example

import (
	"fmt"
	"github.com/lib/pq"
)

func Foo() {
	fmt.Println(pq.ErrSSLNotSupported)
}
`), 0644); err != nil {
		t.Fatal(err)
	}

	deps := discoverDependencies(src, nil)
	if len(deps) != 1 {
		t.Fatalf("expected 1 discovered dep, got %d", len(deps))
	}
	if deps[0].Symbol != "github.com/lib/pq" {
		t.Errorf("expected github.com/lib/pq, got %s", deps[0].Symbol)
	}
	if deps[0].Kind != "unmocked_import" {
		t.Errorf("expected unmocked_import, got %s", deps[0].Kind)
	}
}

func TestDiscoverDependenciesDetectsSubprocessSpawn(t *testing.T) {
	srcDir := t.TempDir()
	src := filepath.Join(srcDir, "target.go")
	if err := os.WriteFile(src, []byte(`package example

import (
	"fmt"
	"os/exec"
)

func Run() {
	cmd := exec.Command("ls")
	fmt.Println(cmd)
}
`), 0644); err != nil {
		t.Fatal(err)
	}

	deps := discoverDependencies(src, nil)
	if len(deps) != 1 {
		t.Fatalf("expected 1 discovered dep, got %d", len(deps))
	}
	if deps[0].Kind != "subprocess_spawn" {
		t.Errorf("expected subprocess_spawn, got %s", deps[0].Kind)
	}
	if !deps[0].IsSubprocessSpawn {
		t.Error("expected IsSubprocessSpawn=true")
	}
}

func TestDiscoverDependenciesExcludesMockedModules(t *testing.T) {
	srcDir := t.TempDir()
	src := filepath.Join(srcDir, "target.go")
	if err := os.WriteFile(src, []byte(`package example

import "github.com/lib/pq"

func Foo() { _ = pq.ErrSSLNotSupported }
`), 0644); err != nil {
		t.Fatal(err)
	}

	mocks := []MockConfig{
		{Symbol: "github.com/lib/pq:Open"},
	}

	deps := discoverDependencies(src, mocks)
	if len(deps) != 0 {
		t.Errorf("expected 0 deps when module is mocked, got %d: %+v", len(deps), deps)
	}
}

func TestDiscoverDependenciesSkipsStdlib(t *testing.T) {
	srcDir := t.TempDir()
	src := filepath.Join(srcDir, "target.go")
	if err := os.WriteFile(src, []byte(`package example

import (
	"fmt"
	"strings"
	"strconv"
)

func Foo() {
	fmt.Println(strings.ToUpper(strconv.Itoa(1)))
}
`), 0644); err != nil {
		t.Fatal(err)
	}

	deps := discoverDependencies(src, nil)
	if len(deps) != 0 {
		t.Errorf("expected 0 deps for stdlib-only imports, got %d: %+v", len(deps), deps)
	}
}
