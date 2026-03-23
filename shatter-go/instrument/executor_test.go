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
	}, true)
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
	}, true)
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
	}, true)
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
	}, true)
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
	}, true)
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
	_, err := ExecuteFunction(src, "nonexistent", nil, true)
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
	}, true)
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
	}, true)
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
	}, true)
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}

	if result.ThrownError == nil {
		t.Fatal("expected error for panicking function")
	}
	// The loop harness catches panics and reports them as "panic" (more precise than
	// the old "runtime_error" which was inferred from non-zero exit status).
	if result.ThrownError.ErrorType != "panic" {
		t.Errorf("expected error_type panic, got %q", result.ThrownError.ErrorType)
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
	}, true)
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
	}, true)
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
	}, true)
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
	}, true)
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

func TestHarnessCacheDirFromEnv(t *testing.T) {
	t.Setenv("SHATTER_HARNESS_CACHE", "/tmp/test-cache")
	if got := harnessCacheDir(); got != "/tmp/test-cache" {
		t.Errorf("expected /tmp/test-cache, got %v", got)
	}
}

func TestHarnessCacheDirUnset(t *testing.T) {
	if got := harnessCacheDir(); got != "" {
		t.Errorf("expected empty string when unset, got %v", got)
	}
}

func TestHarnessScratchDirFromEnv(t *testing.T) {
	t.Setenv("SHATTER_HARNESS_SCRATCH", "/tmp/test-scratch")
	if got := harnessScratchDir(); got != "/tmp/test-scratch" {
		t.Errorf("expected /tmp/test-scratch, got %v", got)
	}
}

func TestHarnessScratchDirUnset(t *testing.T) {
	if got := harnessScratchDir(); got != "" {
		t.Errorf("expected empty string when unset, got %v", got)
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
			DefaultBehavior:  BehaviorRepeatLast,
		},
		{
			Symbol:           "lodash.map",
			ReturnValues:     nil,
			ShouldTrackCalls: false,
			DefaultBehavior:  BehaviorPassthrough,
		},
	}

	source := generateLoopMockFile(mocks)

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
	if !contains(source, "shatterGetAndResetMockCalls") {
		t.Error("expected shatterGetAndResetMockCalls function")
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
	}, true)
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
			DefaultBehavior:  BehaviorThrowError,
		},
	}

	source := generateLoopMockFile(mocks)

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
			DefaultBehavior: BehaviorThrowError,
		},
	}

	source := generateLoopMockFile(mocks)

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

func TestGenerateMockFileThrowErrorGeneratesErrVariant(t *testing.T) {
	mocks := []MockConfig{
		{
			Symbol:           "db.query",
			ReturnValues:     []any{map[string]any{"message": "connection refused"}},
			ShouldTrackCalls: true,
			DefaultBehavior:  BehaviorThrowError,
		},
	}

	source := generateLoopMockFile(mocks)

	// Panic variant
	if !contains(source, "func ShatterMock_db_query(args ...any) any") {
		t.Error("expected panic-variant ShatterMock_db_query")
	}
	if !contains(source, "panic(msg)") {
		t.Error("expected panic(msg) in panic variant")
	}

	// Error-return variant
	if !contains(source, "func ShatterMockErr_db_query(args ...any) (any, error)") {
		t.Error("expected error-return variant ShatterMockErr_db_query")
	}
	if !contains(source, `fmt.Errorf("%s", msg)`) {
		t.Error("expected fmt.Errorf in error-return variant")
	}
}

func TestGenerateMockFileErrVariantImportsFmt(t *testing.T) {
	// With throw_error, "fmt" should be imported.
	mocks := []MockConfig{
		{
			Symbol:          "net.dial",
			DefaultBehavior: BehaviorThrowError,
		},
	}
	source := generateLoopMockFile(mocks)
	if !contains(source, `"fmt"`) {
		t.Error("expected fmt import for throw_error mocks")
	}

	// Without throw_error, "fmt" should NOT be imported.
	mocks2 := []MockConfig{
		{
			Symbol:          "fs.read",
			DefaultBehavior: BehaviorRepeatLast,
		},
	}
	source2 := generateLoopMockFile(mocks2)
	if contains(source2, `"fmt"`) {
		t.Error("fmt import should only appear for throw_error mocks")
	}
}

func TestGenerateMockFileCycleBehavior(t *testing.T) {
	mocks := []MockConfig{
		{
			Symbol:          "cache.get",
			ReturnValues:    []any{"hit", "miss"},
			DefaultBehavior: BehaviorCycle,
		},
	}
	source := generateLoopMockFile(mocks)

	// Cycle behavior uses modulo indexing
	if !contains(source, "idx % len(retvals)") {
		t.Error("expected modulo indexing for cycle behavior")
	}
	// Should NOT contain repeat_last clamping
	if contains(source, "idx >= len(retvals) && len(retvals) > 0") {
		t.Error("cycle behavior should not use repeat_last clamping")
	}
}

func TestGenerateMockFileErrVariantTracksCalls(t *testing.T) {
	// With ShouldTrackCalls=true, error-return variant should track calls.
	mocks := []MockConfig{
		{
			Symbol:           "api.fetch",
			ReturnValues:     []any{map[string]any{"message": "timeout"}},
			ShouldTrackCalls: true,
			DefaultBehavior:  BehaviorThrowError,
		},
	}
	source := generateLoopMockFile(mocks)

	// Count occurrences of shatterRecordMockCall — should appear in both variants.
	count := 0
	for i := 0; i <= len(source)-len(`shatterRecordMockCall("api.fetch"`); i++ {
		if source[i:i+len(`shatterRecordMockCall("api.fetch"`)] == `shatterRecordMockCall("api.fetch"` {
			count++
		}
	}
	if count != 2 {
		t.Errorf("expected shatterRecordMockCall in both panic and error-return variants (got %d occurrences)", count)
	}
}

func TestGenerateMockFileErrVariantNoTrackCalls(t *testing.T) {
	mocks := []MockConfig{
		{
			Symbol:          "api.fetch",
			ReturnValues:    []any{map[string]any{"message": "timeout"}},
			DefaultBehavior: BehaviorThrowError,
		},
	}
	source := generateLoopMockFile(mocks)

	if contains(source, `shatterRecordMockCall("api.fetch"`) {
		t.Error("should not track calls when ShouldTrackCalls is false")
	}
}

func TestGenerateMockFilePerExecutionVariation(t *testing.T) {
	// Simulate two different Execute calls with different mock values.
	mocks1 := []MockConfig{
		{
			Symbol:          "cache.get",
			ReturnValues:    []any{42},
			DefaultBehavior: BehaviorRepeatLast,
		},
	}
	mocks2 := []MockConfig{
		{
			Symbol:          "cache.get",
			ReturnValues:    []any{99},
			DefaultBehavior: BehaviorRepeatLast,
		},
	}

	source1 := generateLoopMockFile(mocks1)
	source2 := generateLoopMockFile(mocks2)

	// The generated sources should differ because return values differ.
	if source1 == source2 {
		t.Error("expected different generated mock files for different return values")
	}
	if !contains(source1, "42") {
		t.Error("expected 42 in first mock file")
	}
	if !contains(source2, "99") {
		t.Error("expected 99 in second mock file")
	}
}

// TestExecuteFunctionStandaloneFileWithMain verifies that a source file containing
// a func main() can be executed — the instrumentation must strip the existing main
// to avoid a redeclaration conflict with the harness main.go.
// Regression test for: build failed: main redeclared in this block
func TestExecuteFunctionStandaloneFileWithMain(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func classify(n int) string {
	if n > 0 {
		return "positive"
	}
	return "non-positive"
}

func main() {}
`)
	result, err := ExecuteFunction(src, "classify", []json.RawMessage{
		json.RawMessage("5"),
	}, true)
	if err != nil {
		t.Fatalf("ExecuteFunction failed on standalone file with main: %v", err)
	}
	if result.ReturnValue == nil {
		t.Fatal("expected non-nil return value")
	}
}

// --- Global state capture tests ---

func TestExecuteFunctionCapturesGlobalStateChange(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

// Counter is an exported package-level variable.
// Expected: incremented from 0 to 1 by increment().
var Counter int = 0

func increment() int {
	Counter++
	return Counter
}
`)
	result, err := ExecuteFunction(src, "increment", []json.RawMessage{}, true)
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}
	if result.ThrownError != nil {
		t.Fatalf("unexpected error: %+v", result.ThrownError)
	}

	var stateChanges []SideEffect
	for _, se := range result.SideEffects {
		if se.Kind == "global_state_change" {
			stateChanges = append(stateChanges, se)
		}
	}
	if len(stateChanges) != 1 {
		t.Fatalf("expected 1 global_state_change side effect, got %d (all side effects: %+v)", len(stateChanges), result.SideEffects)
	}
	sc := stateChanges[0]
	if sc.Variable != "Counter" {
		t.Errorf("expected variable=Counter, got %q", sc.Variable)
	}
	if sc.Before == nil || string(*sc.Before) != "0" {
		t.Errorf("expected before=0, got %v", sc.Before)
	}
	if sc.After == nil || string(*sc.After) != "1" {
		t.Errorf("expected after=1, got %v", sc.After)
	}
}

func TestExecuteFunctionDoesNotReportUnchangedGlobals(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

// Unchanged is never modified by readOnly().
var Unchanged int = 42

func readOnly() int {
	return Unchanged
}
`)
	result, err := ExecuteFunction(src, "readOnly", []json.RawMessage{}, true)
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}

	for _, se := range result.SideEffects {
		if se.Kind == "global_state_change" {
			t.Errorf("unexpected global_state_change for unmodified var: %+v", se)
		}
	}
}

func TestExecuteFunctionDoesNotReportUnexportedGlobals(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

// unexported is not visible to protocol consumers.
var unexported int = 0

func bumpUnexported() int {
	unexported++
	return unexported
}
`)
	result, err := ExecuteFunction(src, "bumpUnexported", []json.RawMessage{}, true)
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}

	for _, se := range result.SideEffects {
		if se.Kind == "global_state_change" {
			t.Errorf("unexpected global_state_change for unexported var: %+v", se)
		}
	}
}

func TestExecuteFunctionCapturesMultipleGlobalStateChanges(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

// Expected: both X and Y are modified by bumpBoth().
var X int = 10
var Y string = "hello"

func bumpBoth() string {
	X = X + 1
	Y = Y + "!"
	return Y
}
`)
	result, err := ExecuteFunction(src, "bumpBoth", []json.RawMessage{}, true)
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}

	changes := make(map[string]SideEffect)
	for _, se := range result.SideEffects {
		if se.Kind == "global_state_change" {
			changes[se.Variable] = se
		}
	}

	if len(changes) != 2 {
		t.Fatalf("expected 2 global_state_change entries, got %d: %+v", len(changes), changes)
	}
	if xSE, ok := changes["X"]; !ok {
		t.Error("missing global_state_change for X")
	} else {
		if string(*xSE.Before) != "10" {
			t.Errorf("X before: want 10, got %s", string(*xSE.Before))
		}
		if string(*xSE.After) != "11" {
			t.Errorf("X after: want 11, got %s", string(*xSE.After))
		}
	}
	if ySE, ok := changes["Y"]; !ok {
		t.Error("missing global_state_change for Y")
	} else {
		if string(*ySE.Before) != `"hello"` {
			t.Errorf("Y before: want \"hello\", got %s", string(*ySE.Before))
		}
		if string(*ySE.After) != `"hello!"` {
			t.Errorf("Y after: want \"hello!\", got %s", string(*ySE.After))
		}
	}
}

// TestAnalyzeGlobalVars verifies the AST-based extractor finds only exported vars.
func TestAnalyzeGlobalVars(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

var Exported int = 0
var AlsoExported string = "x"
var unexported float64 = 3.14

const SomeConst = 42

func someFunc() {}
`)
	vars, err := analyzeGlobalVars(src)
	if err != nil {
		t.Fatalf("analyzeGlobalVars: %v", err)
	}
	names := make(map[string]bool)
	for _, v := range vars {
		names[v.Name] = true
	}
	if !names["Exported"] {
		t.Error("expected Exported to be detected")
	}
	if !names["AlsoExported"] {
		t.Error("expected AlsoExported to be detected")
	}
	if names["unexported"] {
		t.Error("unexported should not be detected")
	}
	if names["SomeConst"] {
		t.Error("constants should not be detected")
	}
	if names["someFunc"] {
		t.Error("functions should not be detected")
	}
}

// ---------------------------------------------------------------------------
// Capture flag tests
// ---------------------------------------------------------------------------

// printingSource is a Go source with a function that writes to stdout.
const printingSource = `package main

import "fmt"

func greetLoud(name string) string {
	fmt.Println("Hello,", name)
	return "done"
}
`

func TestExecuteFunctionCaptureTrueCollectsSideEffects(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", printingSource)

	result, err := ExecuteFunction(src, "greetLoud", []json.RawMessage{
		json.RawMessage(`"world"`),
	}, true)
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}
	if result.ThrownError != nil {
		t.Fatalf("unexpected error: %+v", result.ThrownError)
	}
	if len(result.SideEffects) == 0 {
		t.Error("expected side effects when capture=true, got none")
	}
	found := false
	for _, se := range result.SideEffects {
		if se.Kind == "console_output" {
			found = true
			break
		}
	}
	if !found {
		t.Errorf("expected console_output side effect, got: %+v", result.SideEffects)
	}
}

func TestExecuteFunctionCaptureFalseEmptySideEffects(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", printingSource)

	result, err := ExecuteFunction(src, "greetLoud", []json.RawMessage{
		json.RawMessage(`"world"`),
	}, false)
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}
	if result.ThrownError != nil {
		t.Fatalf("unexpected error: %+v", result.ThrownError)
	}
	if len(result.SideEffects) != 0 {
		t.Errorf("expected empty side_effects when capture=false, got: %+v", result.SideEffects)
	}
	// Non-capture outputs must still be correct.
	var retVal string
	if err := json.Unmarshal(result.ReturnValue, &retVal); err != nil {
		t.Fatalf("parsing return value: %v", err)
	}
	if retVal != "done" {
		t.Errorf("expected return value %q, got %q", "done", retVal)
	}
}

func TestExecuteFunctionCaptureFalsePreservesReturnAndBranches(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

import "fmt"

func classify(x int) string {
	fmt.Println("classifying", x)
	if x > 0 {
		return "positive"
	}
	return "non-positive"
}
`)
	result, err := ExecuteFunction(src, "classify", []json.RawMessage{
		json.RawMessage("5"),
	}, false)
	if err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}
	// Side effects suppressed.
	if len(result.SideEffects) != 0 {
		t.Errorf("expected empty side_effects when capture=false, got: %+v", result.SideEffects)
	}
	// Branch path still populated.
	if len(result.BranchPath) == 0 {
		t.Error("expected branch_path to be populated even when capture=false")
	}
	// Return value still correct.
	var retVal string
	if err := json.Unmarshal(result.ReturnValue, &retVal); err != nil {
		t.Fatalf("parsing return value: %v", err)
	}
	if retVal != "positive" {
		t.Errorf("expected %q, got %q", "positive", retVal)
	}
}

// --- Persistent subprocess tests ---

// TestPersistentHarnessReusesSameSubprocess verifies that two sequential calls to the
// same function reuse the cached harness rather than recompiling.
func TestPersistentHarnessReusesSameSubprocess(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func double(n int) int {
	return n * 2
}
`)
	// First call — cold path: compiles and spawns harness.
	result1, err := ExecuteFunction(src, "double", []json.RawMessage{json.RawMessage("5")}, false)
	if err != nil {
		t.Fatalf("first call: %v", err)
	}
	var v1 int
	if err := json.Unmarshal(result1.ReturnValue, &v1); err != nil {
		t.Fatalf("unmarshal result1: %v", err)
	}
	if v1 != 10 {
		t.Errorf("first call: expected 10, got %d", v1)
	}

	// Retrieve the harness from cache and record its pid.
	id := harnessID{sourcePath: src, funcName: "double", mocksHash: ""}
	h1 := getHarness(id)
	if h1 == nil {
		t.Fatal("expected harness in cache after first call")
	}
	pid1 := h1.cmd.Process.Pid

	// Second call — warm path: reuses existing subprocess.
	result2, err := ExecuteFunction(src, "double", []json.RawMessage{json.RawMessage("7")}, false)
	if err != nil {
		t.Fatalf("second call: %v", err)
	}
	var v2 int
	if err := json.Unmarshal(result2.ReturnValue, &v2); err != nil {
		t.Fatalf("unmarshal result2: %v", err)
	}
	if v2 != 14 {
		t.Errorf("second call: expected 14, got %d", v2)
	}

	h2 := getHarness(id)
	if h2 == nil {
		t.Fatal("expected harness in cache after second call")
	}
	if h2.cmd.Process.Pid != pid1 {
		t.Errorf("second call spawned a new subprocess (pid %d → %d), expected reuse", pid1, h2.cmd.Process.Pid)
	}

	// Cleanup
	CloseAllHarnesses()
}

// TestPersistentHarnessResultsDoNotAccumulate verifies that branch recordings from
// one iteration do not bleed into the next (state is reset between calls).
func TestPersistentHarnessResultsDoNotAccumulate(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func classify(n int) string {
	if n > 0 {
		return "positive"
	}
	return "non-positive"
}
`)
	// Call once with a positive value (true branch taken).
	r1, err := ExecuteFunction(src, "classify", []json.RawMessage{json.RawMessage("5")}, false)
	if err != nil {
		t.Fatalf("call 1: %v", err)
	}
	branches1 := len(r1.BranchPath)

	// Call again with negative value (false branch taken).
	r2, err := ExecuteFunction(src, "classify", []json.RawMessage{json.RawMessage("-3")}, false)
	if err != nil {
		t.Fatalf("call 2: %v", err)
	}
	branches2 := len(r2.BranchPath)

	// Each call should record exactly 1 branch decision, not 2 accumulated.
	if branches1 != 1 {
		t.Errorf("call 1: expected 1 branch decision, got %d", branches1)
	}
	if branches2 != 1 {
		t.Errorf("call 2: expected 1 branch decision, got %d", branches2)
	}

	// Branch decisions should differ between calls.
	if branches1 > 0 && branches2 > 0 {
		if r1.BranchPath[0].Taken == r2.BranchPath[0].Taken {
			t.Errorf("expected different branch outcomes for n=5 and n=-3")
		}
	}

	CloseAllHarnesses()
}

// TestCloseAllHarnesses verifies that CloseAllHarnesses terminates subprocesses and
// clears the cache.
func TestCloseAllHarnesses(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func inc(n int) int { return n + 1 }
`)
	if _, err := ExecuteFunction(src, "inc", []json.RawMessage{json.RawMessage("1")}, false); err != nil {
		t.Fatalf("ExecuteFunction: %v", err)
	}
	id := harnessID{sourcePath: src, funcName: "inc", mocksHash: ""}
	if getHarness(id) == nil {
		t.Fatal("expected harness in cache")
	}

	CloseAllHarnesses()

	if getHarness(id) != nil {
		t.Error("expected cache empty after CloseAllHarnesses")
	}
}

// TestPersistentHarnessCrashRecovery verifies that after a harness crash the next
// call recompiles and succeeds.
func TestPersistentHarnessCrashRecovery(t *testing.T) {
	srcDir := t.TempDir()
	src := writeExecTestSource(t, srcDir, "target.go", `package main

func inc(n int) int { return n + 1 }
`)
	// First call succeeds.
	if _, err := ExecuteFunction(src, "inc", []json.RawMessage{json.RawMessage("1")}, false); err != nil {
		t.Fatalf("first call: %v", err)
	}

	// Simulate crash by killing the subprocess's stdin (causes EOF in harness loop).
	id := harnessID{sourcePath: src, funcName: "inc", mocksHash: ""}
	h := getHarness(id)
	if h == nil {
		t.Fatal("expected harness in cache")
	}
	h.stdin.Close() // sends EOF → harness exits
	// Give it a moment to exit
	time.Sleep(50 * time.Millisecond)

	// Manually remove the dead entry so the next call re-spawns.
	removeHarness(id)

	// Second call should recompile and succeed.
	result, err := ExecuteFunction(src, "inc", []json.RawMessage{json.RawMessage("41")}, false)
	if err != nil {
		t.Fatalf("recovery call: %v", err)
	}
	var v int
	if err := json.Unmarshal(result.ReturnValue, &v); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if v != 42 {
		t.Errorf("expected 42, got %d", v)
	}

	CloseAllHarnesses()
}
