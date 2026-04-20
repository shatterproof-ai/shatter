package workspace

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestNewRunWritesRunningMetadata(t *testing.T) {
	workspace := openWorkspace(t)

	run, err := workspace.NewRun([]string{"explore", "examples/go/04-nested-control-flow.go"})
	if err != nil {
		t.Fatalf("NewRun: %v", err)
	}
	if run.ID() == "" {
		t.Fatal("run.ID() should not be empty")
	}
	if info, err := os.Stat(run.Path()); err != nil || !info.IsDir() {
		t.Fatalf("run path %q should be a directory: %v", run.Path(), err)
	}

	metadata := readRunMetadata(t, run.Path())
	if metadata.SchemaVersion != runMetadataSchemaVersion {
		t.Fatalf("schema_version = %d, want %d", metadata.SchemaVersion, runMetadataSchemaVersion)
	}
	if metadata.Status != RunStatusRunning {
		t.Fatalf("status = %q, want %q", metadata.Status, RunStatusRunning)
	}
	if metadata.RunID != run.ID() {
		t.Fatalf("run_id = %q, want %q", metadata.RunID, run.ID())
	}
	if metadata.EndTime != "" {
		t.Fatalf("end_time should be empty while running, got %q", metadata.EndTime)
	}
	wantArgs := []string{"explore", "examples/go/04-nested-control-flow.go"}
	if len(metadata.Args) != len(wantArgs) || metadata.Args[0] != wantArgs[0] || metadata.Args[1] != wantArgs[1] {
		t.Fatalf("args = %v, want %v", metadata.Args, wantArgs)
	}
}

func TestFinishUpdatesStatus(t *testing.T) {
	workspace := openWorkspace(t)

	run, err := workspace.NewRun(nil)
	if err != nil {
		t.Fatalf("NewRun: %v", err)
	}
	if err := run.Finish(RunStatusCompleted); err != nil {
		t.Fatalf("Finish: %v", err)
	}

	metadata := readRunMetadata(t, run.Path())
	if metadata.Status != RunStatusCompleted {
		t.Fatalf("status = %q, want %q", metadata.Status, RunStatusCompleted)
	}
	if metadata.EndTime == "" {
		t.Fatal("end_time should be set after Finish")
	}
}

func TestFinishRejectsUnknownStatus(t *testing.T) {
	workspace := openWorkspace(t)

	run, err := workspace.NewRun(nil)
	if err != nil {
		t.Fatalf("NewRun: %v", err)
	}
	if err := run.Finish("weird"); err == nil {
		t.Fatal("Finish with unknown status should error")
	}
}

func TestListRunsSortsByStartTime(t *testing.T) {
	workspace := openWorkspace(t)

	times := []time.Time{
		time.Date(2026, 3, 1, 0, 0, 0, 0, time.UTC),
		time.Date(2026, 1, 1, 0, 0, 0, 0, time.UTC),
		time.Date(2026, 2, 1, 0, 0, 0, 0, time.UTC),
	}
	for index, start := range times {
		seedRun(t, workspace, index, start, RunStatusCompleted)
	}

	runs, err := workspace.ListRuns()
	if err != nil {
		t.Fatalf("ListRuns: %v", err)
	}
	if len(runs) != 3 {
		t.Fatalf("got %d runs, want 3", len(runs))
	}
	for index := 1; index < len(runs); index++ {
		if runs[index].StartTime.Before(runs[index-1].StartTime) {
			t.Fatalf("ListRuns not sorted ascending at index %d: %v before %v",
				index, runs[index].StartTime, runs[index-1].StartTime)
		}
	}
}

func TestListRunsTolerantOfCorruptMetadata(t *testing.T) {
	workspace := openWorkspace(t)

	corruptDir := filepath.Join(workspace.RunsDir(), "corrupt-run")
	if err := os.MkdirAll(corruptDir, 0o755); err != nil {
		t.Fatalf("mkdir corrupt: %v", err)
	}
	if err := os.WriteFile(filepath.Join(corruptDir, metadataFileName), []byte("not json"), 0o644); err != nil {
		t.Fatalf("write corrupt metadata: %v", err)
	}

	runs, err := workspace.ListRuns()
	if err != nil {
		t.Fatalf("ListRuns: %v", err)
	}
	if len(runs) != 1 {
		t.Fatalf("got %d runs, want 1", len(runs))
	}
	if runs[0].HasMetadata {
		t.Fatal("corrupt run should have HasMetadata=false")
	}
	if runs[0].RunID != "corrupt-run" {
		t.Fatalf("RunID = %q, want corrupt-run", runs[0].RunID)
	}
	if runs[0].StartTime.IsZero() {
		t.Fatal("StartTime should fall back to dir mtime, not zero")
	}
}

func openWorkspace(t *testing.T) *Workspace {
	t.Helper()
	root := filepath.Join(t.TempDir(), "workspace-root")
	workspace, err := Open(root)
	if err != nil {
		t.Fatalf("Open: %v", err)
	}
	if err := workspace.Ensure(); err != nil {
		t.Fatalf("Ensure: %v", err)
	}
	return workspace
}

func readRunMetadata(t *testing.T, runPath string) RunMetadata {
	t.Helper()
	bytes, err := os.ReadFile(filepath.Join(runPath, metadataFileName))
	if err != nil {
		t.Fatalf("read metadata: %v", err)
	}
	var metadata RunMetadata
	if err := json.Unmarshal(bytes, &metadata); err != nil {
		t.Fatalf("unmarshal metadata: %v", err)
	}
	return metadata
}

// seedRun creates a synthetic run directory with controlled start time and
// size (in bytes) and returns its RunInfo-style path. Used by gc tests.
func seedRun(t *testing.T, workspace *Workspace, index int, start time.Time, status string) string {
	t.Helper()
	return seedRunWithSize(t, workspace, index, start, status, 0)
}

func seedRunWithSize(t *testing.T, workspace *Workspace, index int, start time.Time, status string, sizeBytes int) string {
	t.Helper()
	runID := start.UTC().Format("20060102T150405Z") + "-seed" + string(rune('a'+index%26))
	runPath := filepath.Join(workspace.RunsDir(), runID)
	if err := os.MkdirAll(runPath, 0o755); err != nil {
		t.Fatalf("mkdir seed run: %v", err)
	}
	metadata := RunMetadata{
		SchemaVersion: runMetadataSchemaVersion,
		RunID:         runID,
		StartTime:     start.Format(time.RFC3339Nano),
		Args:          []string{"seed"},
		Status:        status,
	}
	if status == RunStatusCompleted || status == RunStatusFailed {
		metadata.EndTime = start.Add(time.Minute).Format(time.RFC3339Nano)
	}
	bytes, err := json.MarshalIndent(metadata, "", "  ")
	if err != nil {
		t.Fatalf("marshal seed metadata: %v", err)
	}
	if err := os.WriteFile(filepath.Join(runPath, metadataFileName), append(bytes, '\n'), 0o644); err != nil {
		t.Fatalf("write seed metadata: %v", err)
	}
	if sizeBytes > 0 {
		filler := make([]byte, sizeBytes)
		if err := os.WriteFile(filepath.Join(runPath, "filler.bin"), filler, 0o644); err != nil {
			t.Fatalf("write filler: %v", err)
		}
	}
	return runPath
}
