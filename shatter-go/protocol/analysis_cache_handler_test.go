package protocol

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/shatter-dev/shatter/shatter-go/workspace"
)

// cacheHandlerFixture wires a Handler against a tmp workspace and a
// captured-stderr buffer so tests can assert against emitted log lines.
type cacheHandlerFixture struct {
	handler  *Handler
	workspace *workspace.Workspace
	logBuffer *bytes.Buffer
	stdin     *bytes.Buffer
	stdout    *bytes.Buffer
}

func newCacheHandlerFixture(t *testing.T) *cacheHandlerFixture {
	t.Helper()
	root := t.TempDir()
	ws, err := workspace.Initialize(workspace.ResolveOptions{RepoOverrideRoot: root})
	if err != nil {
		t.Fatalf("initialize workspace: %v", err)
	}
	logBuffer := &bytes.Buffer{}
	stdin := &bytes.Buffer{}
	stdout := &bytes.Buffer{}
	handler := NewHandlerWithWorkspace(stdin, stdout, logBuffer, ws)
	return &cacheHandlerFixture{
		handler:   handler,
		workspace: ws,
		logBuffer: logBuffer,
		stdin:     stdin,
		stdout:    stdout,
	}
}

// runAnalyze invokes handleAnalyze directly on the fixture's handler with the
// given file path. Bypasses the JSON request loop so failures surface cleanly.
func (f *cacheHandlerFixture) runAnalyze(t *testing.T, file string) Response {
	t.Helper()
	resp := Response{
		ProtocolVersion: ProtocolVersion,
		FrontendVersion: frontendVersion,
	}
	return f.handler.handleAnalyze(resp, Request{File: file, Command: "analyze"})
}

// drainLogs returns and clears the captured log buffer.
func (f *cacheHandlerFixture) drainLogs() string {
	contents := f.logBuffer.String()
	f.logBuffer.Reset()
	return contents
}

// writePackage materializes a tiny analyzable Go package under root and
// returns the path of the target file.
func writePackage(t *testing.T, root string) string {
	t.Helper()
	pkgDir := filepath.Join(root, "p")
	if err := os.MkdirAll(pkgDir, 0o755); err != nil {
		t.Fatalf("mkdir: %v", err)
	}
	if err := os.WriteFile(filepath.Join(root, "go.mod"), []byte("module example.com/m\n\ngo 1.23\n"), 0o644); err != nil {
		t.Fatalf("write go.mod: %v", err)
	}
	target := filepath.Join(pkgDir, "f.go")
	body := `package p

func F(x int) int {
	if x > 0 {
		return x
	}
	return -x
}
`
	if err := os.WriteFile(target, []byte(body), 0o644); err != nil {
		t.Fatalf("write target: %v", err)
	}
	return target
}

// Two consecutive analyses of the same package: first is a miss + write,
// second is a hit. Asserts the C6 acceptance criterion via captured logs.
func TestHandleAnalyze_CacheHitOnSecondCall(t *testing.T) {
	f := newCacheHandlerFixture(t)
	target := writePackage(t, t.TempDir())

	// First call: miss + write.
	resp1 := f.runAnalyze(t, target)
	if resp1.Status != "analyze" {
		t.Fatalf("first analyze status = %q, msg = %q", resp1.Status, resp1.Message)
	}
	logs1 := f.drainLogs()
	if !strings.Contains(logs1, "analysis cache miss") {
		t.Errorf("first call should log a cache miss; logs:\n%s", logs1)
	}
	if strings.Contains(logs1, "analysis cache hit") {
		t.Errorf("first call must not log a cache hit; logs:\n%s", logs1)
	}

	// Second call: hit (no analyzer cost).
	resp2 := f.runAnalyze(t, target)
	if resp2.Status != "analyze" {
		t.Fatalf("second analyze status = %q, msg = %q", resp2.Status, resp2.Message)
	}
	logs2 := f.drainLogs()
	if !strings.Contains(logs2, "analysis cache hit") {
		t.Errorf("second call should log a cache hit; logs:\n%s", logs2)
	}
	if strings.Contains(logs2, "analysis cache miss") {
		t.Errorf("second call must not log a cache miss; logs:\n%s", logs2)
	}

	// Cache-hit response must carry the same set of analyzed function names
	// as the first response — invariant check on the round-trip.
	if !sameFunctionNames(resp1.Functions, resp2.Functions) {
		t.Errorf("cache-hit response diverged: first=%v second=%v",
			functionNames(resp1.Functions), functionNames(resp2.Functions))
	}
}

// A single-line source change must invalidate the cache: the third call,
// after rewriting the target, must log a miss.
func TestHandleAnalyze_SingleLineSourceChangeInvalidates(t *testing.T) {
	f := newCacheHandlerFixture(t)
	target := writePackage(t, t.TempDir())

	// Prime the cache.
	if resp := f.runAnalyze(t, target); resp.Status != "analyze" {
		t.Fatalf("prime status = %q msg=%q", resp.Status, resp.Message)
	}
	f.drainLogs()
	if resp := f.runAnalyze(t, target); resp.Status != "analyze" {
		t.Fatalf("warm status = %q msg=%q", resp.Status, resp.Message)
	}
	if logs := f.drainLogs(); !strings.Contains(logs, "analysis cache hit") {
		t.Fatalf("expected cache hit before mutation; logs:\n%s", logs)
	}

	// Mutate one line and re-analyze.
	mutated := `package p

func F(x int) int {
	if x >= 0 {
		return x
	}
	return -x
}
`
	if err := os.WriteFile(target, []byte(mutated), 0o644); err != nil {
		t.Fatalf("rewrite target: %v", err)
	}

	if resp := f.runAnalyze(t, target); resp.Status != "analyze" {
		t.Fatalf("post-mutation status = %q msg=%q", resp.Status, resp.Message)
	}
	logs := f.drainLogs()
	if !strings.Contains(logs, "analysis cache miss") {
		t.Errorf("post-mutation call must log a miss; logs:\n%s", logs)
	}
	if strings.Contains(logs, "analysis cache hit") {
		t.Errorf("post-mutation call must not log a hit; logs:\n%s", logs)
	}
}

// SHATTER_DISABLE_ANALYSIS_CACHE=1 makes every call a miss, regardless of
// previously-written payloads.
func TestHandleAnalyze_DisableEnvVarSkipsCache(t *testing.T) {
	f := newCacheHandlerFixture(t)
	target := writePackage(t, t.TempDir())

	// Prime cache.
	f.runAnalyze(t, target)
	f.drainLogs()

	t.Setenv(analysisCacheDisableEnvVar, "1")
	if resp := f.runAnalyze(t, target); resp.Status != "analyze" {
		t.Fatalf("disabled-cache analyze status = %q msg=%q", resp.Status, resp.Message)
	}
	logs := f.drainLogs()
	if !strings.Contains(logs, `reason=disabled`) {
		t.Errorf("expected disabled-miss reason; logs:\n%s", logs)
	}
	if strings.Contains(logs, "analysis cache hit") {
		t.Errorf("disabled cache must not produce a hit; logs:\n%s", logs)
	}
}

// Cache payload on disk must be valid JSON with the agreed shape.
func TestHandleAnalyze_CachePayloadShape(t *testing.T) {
	f := newCacheHandlerFixture(t)
	target := writePackage(t, t.TempDir())

	if resp := f.runAnalyze(t, target); resp.Status != "analyze" {
		t.Fatalf("analyze status = %q msg=%q", resp.Status, resp.Message)
	}

	entries, err := os.ReadDir(f.workspace.AnalysisDir())
	if err != nil {
		t.Fatalf("read analysis dir: %v", err)
	}
	var jsonFiles []string
	for _, entry := range entries {
		if filepath.Ext(entry.Name()) == analysisCacheFileExtension {
			jsonFiles = append(jsonFiles, entry.Name())
		}
	}
	if len(jsonFiles) != 1 {
		t.Fatalf("expected exactly one cache file, got %d: %v", len(jsonFiles), entries)
	}
	bytes, err := os.ReadFile(filepath.Join(f.workspace.AnalysisDir(), jsonFiles[0]))
	if err != nil {
		t.Fatalf("read cache file: %v", err)
	}
	var payload analysisCachePayload
	if err := json.Unmarshal(bytes, &payload); err != nil {
		t.Fatalf("unmarshal cache payload: %v", err)
	}
	if payload.SchemaVersion != analysisCacheSchemaVersion {
		t.Errorf("schema_version = %d, want %d", payload.SchemaVersion, analysisCacheSchemaVersion)
	}
	if payload.ShatterVersion != ProtocolVersion {
		t.Errorf("shatter_version = %q, want %q", payload.ShatterVersion, ProtocolVersion)
	}
	if payload.SourcePath != target {
		t.Errorf("source_path = %q, want %q", payload.SourcePath, target)
	}
	if _, err := time.Parse(time.RFC3339, payload.CreatedAt); err != nil {
		t.Errorf("created_at not RFC3339: %q (%v)", payload.CreatedAt, err)
	}
	if len(payload.Functions) == 0 {
		t.Errorf("expected at least one analyzed function, got 0")
	}
}

// Suppress unused-import warnings if io is used only in test-helper code we
// might add later.
var _ io.Writer = (*bytes.Buffer)(nil)

func functionNames(fs []FunctionAnalysis) []string {
	names := make([]string, 0, len(fs))
	for _, f := range fs {
		names = append(names, f.Name)
	}
	return names
}

func sameFunctionNames(a, b []FunctionAnalysis) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i].Name != b[i].Name {
			return false
		}
	}
	return true
}

// Defensive: ensure fmt is used (avoid unused import errors if we trim
// later). The variable is used in error messages above.
var _ = fmt.Sprintf
