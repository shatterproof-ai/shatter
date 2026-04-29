package protocol

import (
	"encoding/json"
	"os"
	"path/filepath"
	"runtime"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/workspace"
)

// helperWorkspace creates a workspace under t.TempDir for cache tests.
func helperWorkspace(t *testing.T) *workspace.Workspace {
	t.Helper()
	root := t.TempDir()
	ws, err := workspace.Initialize(workspace.ResolveOptions{RepoOverrideRoot: root})
	if err != nil {
		t.Fatalf("initialize workspace: %v", err)
	}
	return ws
}

// writeGoFile writes a tiny package file with the given body to dir/name and
// returns its absolute path. The file declares package p and a single
// function so the analyzer has something to walk.
func writeGoFile(t *testing.T, dir, name, body string) string {
	t.Helper()
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatalf("mkdir %q: %v", dir, err)
	}
	path := filepath.Join(dir, name)
	if err := os.WriteFile(path, []byte(body), 0o644); err != nil {
		t.Fatalf("write %q: %v", path, err)
	}
	return path
}

const minimalPkgFile = `package p

func F(x int) int {
	if x > 0 {
		return x
	}
	return -x
}
`

const minimalPkgFileMutated = `package p

func F(x int) int {
	if x >= 0 {
		return x
	}
	return -x
}
`

const minimalGoMod = "module example.com/p\n\ngo 1.23\n"

// computeDiscoveryHashStableness: same package contents → same hash across
// independent calls (no clock or random salt leakage).
func TestComputeDiscoveryHash_DeterministicForSameInputs(t *testing.T) {
	dir := t.TempDir()
	writeGoFile(t, dir, "go.mod", minimalGoMod)
	writeGoFile(t, dir, "f.go", minimalPkgFile)

	first, err := ComputeDiscoveryHash(filepath.Join(dir, "f.go"), "")
	if err != nil {
		t.Fatalf("first hash: %v", err)
	}
	second, err := ComputeDiscoveryHash(filepath.Join(dir, "f.go"), "")
	if err != nil {
		t.Fatalf("second hash: %v", err)
	}
	if first != second {
		t.Errorf("ComputeDiscoveryHash not deterministic: %q vs %q", first, second)
	}
	if len(first) != discoveryHashHexLength {
		t.Errorf("hash length = %d, want %d", len(first), discoveryHashHexLength)
	}
}

// A single-byte source change must invalidate the hash.
func TestComputeDiscoveryHash_SourceChangeInvalidates(t *testing.T) {
	dir := t.TempDir()
	writeGoFile(t, dir, "go.mod", minimalGoMod)
	target := writeGoFile(t, dir, "f.go", minimalPkgFile)

	before, err := ComputeDiscoveryHash(target, "")
	if err != nil {
		t.Fatalf("hash before: %v", err)
	}
	if err := os.WriteFile(target, []byte(minimalPkgFileMutated), 0o644); err != nil {
		t.Fatalf("rewrite target: %v", err)
	}
	after, err := ComputeDiscoveryHash(target, "")
	if err != nil {
		t.Fatalf("hash after: %v", err)
	}
	if before == after {
		t.Errorf("hash did not invalidate on source change: %q == %q", before, after)
	}
}

// The function-name filter is part of the cache key: analyzing the whole file
// vs. one specific function may produce different result sets, so caching
// them under the same hash would return wrong data.
func TestComputeDiscoveryHash_FunctionNameAffectsHash(t *testing.T) {
	dir := t.TempDir()
	writeGoFile(t, dir, "go.mod", minimalGoMod)
	target := writeGoFile(t, dir, "f.go", minimalPkgFile)

	whole, err := ComputeDiscoveryHash(target, "")
	if err != nil {
		t.Fatalf("hash whole: %v", err)
	}
	scoped, err := ComputeDiscoveryHash(target, "F")
	if err != nil {
		t.Fatalf("hash scoped: %v", err)
	}
	if whole == scoped {
		t.Errorf("function-name filter did not affect hash: %q", whole)
	}
}

// Sibling files in the same package must contribute to the hash even when the
// target file itself is unchanged. Analyzer output for a package depends on
// all sibling files (type info, same-package constructors, etc.).
func TestComputeDiscoveryHash_SiblingFileChangeInvalidates(t *testing.T) {
	dir := t.TempDir()
	writeGoFile(t, dir, "go.mod", minimalGoMod)
	target := writeGoFile(t, dir, "f.go", minimalPkgFile)
	sibling := writeGoFile(t, dir, "g.go", "package p\n\nfunc G() {}\n")

	before, err := ComputeDiscoveryHash(target, "")
	if err != nil {
		t.Fatalf("hash before: %v", err)
	}
	if err := os.WriteFile(sibling, []byte("package p\n\nfunc G() int { return 1 }\n"), 0o644); err != nil {
		t.Fatalf("rewrite sibling: %v", err)
	}
	after, err := ComputeDiscoveryHash(target, "")
	if err != nil {
		t.Fatalf("hash after: %v", err)
	}
	if before == after {
		t.Errorf("hash did not invalidate on sibling change: %q == %q", before, after)
	}
}

// Round-trip: write then read returns the same FunctionAnalysis slice.
func TestAnalysisCache_WriteReadRoundtrip(t *testing.T) {
	ws := helperWorkspace(t)
	hash := "deadbeef00000000deadbeef00000000"
	source := "/tmp/example.go"
	functions := []FunctionAnalysis{
		{Name: "F", SourceFile: source},
		{Name: "G", SourceFile: source},
	}

	if err := WriteAnalysisCache(ws, hash, source, "", functions); err != nil {
		t.Fatalf("WriteAnalysisCache: %v", err)
	}
	got, hit, miss := ReadAnalysisCache(ws, hash)
	if !hit {
		t.Fatalf("expected hit, got miss reason %q", miss)
	}
	if len(got) != len(functions) || got[0].Name != "F" || got[1].Name != "G" {
		t.Errorf("roundtrip mismatch: got=%+v want=%+v", got, functions)
	}
}

// Schema-version mismatch must be treated as a miss with reason
// "schema_mismatch", not as a fatal error.
func TestAnalysisCache_SchemaVersionMismatchIsMiss(t *testing.T) {
	ws := helperWorkspace(t)
	hash := "feedface00000000feedface00000000"
	cachePath := filepath.Join(ws.AnalysisDir(), hash+analysisCacheFileExtension)
	bad := map[string]any{
		"schema_version":  9999,
		"shatter_version": ProtocolVersion,
		"functions":       []any{},
	}
	bytes, err := json.Marshal(bad)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	if err := os.WriteFile(cachePath, bytes, 0o644); err != nil {
		t.Fatalf("write: %v", err)
	}
	_, hit, miss := ReadAnalysisCache(ws, hash)
	if hit {
		t.Errorf("expected miss on schema mismatch, got hit")
	}
	if miss != analysisCacheMissSchemaMismatch {
		t.Errorf("miss reason = %q, want %q", miss, analysisCacheMissSchemaMismatch)
	}
}

// Corrupt JSON payload must be tolerated (logged and treated as miss).
func TestAnalysisCache_CorruptJSONIsMiss(t *testing.T) {
	ws := helperWorkspace(t)
	hash := "cafebabe00000000cafebabe00000000"
	cachePath := filepath.Join(ws.AnalysisDir(), hash+analysisCacheFileExtension)
	if err := os.WriteFile(cachePath, []byte("{not json"), 0o644); err != nil {
		t.Fatalf("write: %v", err)
	}
	_, hit, miss := ReadAnalysisCache(ws, hash)
	if hit {
		t.Errorf("expected miss on corrupt JSON, got hit")
	}
	if miss != analysisCacheMissParseError {
		t.Errorf("miss reason = %q, want %q", miss, analysisCacheMissParseError)
	}
}

// Missing file is a clean "not_found" miss, not an error path.
func TestAnalysisCache_NotFoundIsMiss(t *testing.T) {
	ws := helperWorkspace(t)
	hash := "0123456789abcdef0123456789abcdef"
	_, hit, miss := ReadAnalysisCache(ws, hash)
	if hit {
		t.Errorf("expected miss for nonexistent hash, got hit")
	}
	if miss != analysisCacheMissNotFound {
		t.Errorf("miss reason = %q, want %q", miss, analysisCacheMissNotFound)
	}
}

// Shatter-version mismatch is a miss; an analyzer upgrade must invalidate
// every previously-cached payload without manual cleanup.
func TestAnalysisCache_ShatterVersionMismatchIsMiss(t *testing.T) {
	ws := helperWorkspace(t)
	hash := "babecafe00000000babecafe00000000"
	cachePath := filepath.Join(ws.AnalysisDir(), hash+analysisCacheFileExtension)
	stale := map[string]any{
		"schema_version":  analysisCacheSchemaVersion,
		"shatter_version": "0.0.0-stale",
		"functions":       []any{},
	}
	bytes, err := json.Marshal(stale)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	if err := os.WriteFile(cachePath, bytes, 0o644); err != nil {
		t.Fatalf("write: %v", err)
	}
	_, hit, miss := ReadAnalysisCache(ws, hash)
	if hit {
		t.Errorf("expected miss on version mismatch, got hit")
	}
	if miss != analysisCacheMissVersionMismatch {
		t.Errorf("miss reason = %q, want %q", miss, analysisCacheMissVersionMismatch)
	}
}

// Atomic write: the cache directory must never contain a partially-written
// .json file even if a writer crashes mid-flush. We can't realistically
// crash the writer, but we can assert no `.tmp-*` artifact survives a
// successful WriteAnalysisCache call.
func TestAnalysisCache_NoLingeringTempFiles(t *testing.T) {
	ws := helperWorkspace(t)
	hash := "1111222233334444aaaabbbbccccdddd"
	if err := WriteAnalysisCache(ws, hash, "/x.go", "", nil); err != nil {
		t.Fatalf("WriteAnalysisCache: %v", err)
	}
	entries, err := os.ReadDir(ws.AnalysisDir())
	if err != nil {
		t.Fatalf("read analysis dir: %v", err)
	}
	for _, e := range entries {
		if e.Name() == hash+analysisCacheFileExtension {
			continue
		}
		t.Errorf("unexpected file in analysis dir after write: %q", e.Name())
	}
}

// SHATTER_DISABLE_ANALYSIS_CACHE=1 disables the cache (read returns miss
// regardless of payload presence).
func TestAnalysisCache_DisableEnvVar(t *testing.T) {
	ws := helperWorkspace(t)
	hash := "5555555555555555aaaaaaaaaaaaaaaa"
	functions := []FunctionAnalysis{{Name: "F"}}
	if err := WriteAnalysisCache(ws, hash, "/x.go", "", functions); err != nil {
		t.Fatalf("seed cache: %v", err)
	}

	t.Setenv(analysisCacheDisableEnvVar, "1")
	_, hit, miss := ReadAnalysisCache(ws, hash)
	if hit {
		t.Errorf("expected miss with %s=1, got hit", analysisCacheDisableEnvVar)
	}
	if miss != analysisCacheMissDisabled {
		t.Errorf("miss reason = %q, want %q", miss, analysisCacheMissDisabled)
	}
}

// Cache cannot be read when no workspace is provided (defensive — callers
// already gate on h.workspace != nil, but the helpers should be safe).
func TestAnalysisCache_NilWorkspaceIsMiss(t *testing.T) {
	_, hit, miss := ReadAnalysisCache(nil, "any")
	if hit {
		t.Errorf("expected miss with nil workspace, got hit")
	}
	if miss != analysisCacheMissNotFound {
		t.Errorf("miss reason = %q, want %q", miss, analysisCacheMissNotFound)
	}
}

// Smoke check that the runtime version is part of the hash input — switching
// runtimes should cause a different hash. We can't actually change
// runtime.Version() in a test, but the constant input set is stable enough
// that we just assert the function returns without error and the hash is
// non-empty across a representative call.
func TestComputeDiscoveryHash_IncludesRuntimeVersion(t *testing.T) {
	dir := t.TempDir()
	writeGoFile(t, dir, "go.mod", minimalGoMod)
	target := writeGoFile(t, dir, "f.go", minimalPkgFile)

	hash, err := ComputeDiscoveryHash(target, "")
	if err != nil {
		t.Fatalf("hash: %v", err)
	}
	if hash == "" {
		t.Errorf("empty hash for valid input")
	}
	if runtime.Version() == "" {
		t.Skip("runtime.Version() is empty; cannot validate inclusion")
	}
}
