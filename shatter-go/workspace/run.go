package workspace

import (
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"time"
)

const (
	runMetadataSchemaVersion = 1

	RunStatusRunning   = "running"
	RunStatusCompleted = "completed"
	RunStatusFailed    = "failed"

	runIDSuffixBytes = 3 // 6 hex chars
)

// RunMetadata is the on-disk schema for runs/<runID>/metadata.json.
type RunMetadata struct {
	SchemaVersion int      `json:"schema_version"`
	RunID         string   `json:"run_id"`
	StartTime     string   `json:"start_time"`
	EndTime       string   `json:"end_time,omitempty"`
	Args          []string `json:"args"`
	Status        string   `json:"status"`
}

// RunInfo is a parsed or reconstructed view of a single run directory used by
// gc and inspection callers. When the run's metadata.json is missing or
// unparseable, StartTime falls back to the directory's modification time and
// the rest of the fields are zero-valued.
type RunInfo struct {
	RunID        string
	Path         string
	StartTime    time.Time
	HasMetadata  bool
	Metadata     RunMetadata
	SizeBytes    int64
}

// Run is an in-flight run handle. Created by NewRun, finalized by Finish.
type Run struct {
	workspace *Workspace
	id        string
	path      string
	args      []string
	start     time.Time
}

// NewRun generates a run ID, creates runs/<runID>/, and writes metadata.json
// with status="running". The args slice is captured as-is for future gc and
// inspection callers.
func (w *Workspace) NewRun(args []string) (*Run, error) {
	if err := os.MkdirAll(w.RunsDir(), 0o755); err != nil {
		return nil, fmt.Errorf("mkdir runs: %w", err)
	}

	start := time.Now().UTC()
	runID, err := generateRunID(start)
	if err != nil {
		return nil, err
	}
	path := filepath.Join(w.RunsDir(), runID)
	if err := os.MkdirAll(path, 0o755); err != nil {
		return nil, fmt.Errorf("mkdir run %s: %w", runID, err)
	}

	capturedArgs := append([]string(nil), args...)
	run := &Run{
		workspace: w,
		id:        runID,
		path:      path,
		args:      capturedArgs,
		start:     start,
	}
	if err := run.writeMetadata(RunStatusRunning, time.Time{}); err != nil {
		return nil, err
	}
	return run, nil
}

// ID returns the run ID.
func (r *Run) ID() string { return r.id }

// Path returns the absolute path of runs/<runID>/.
func (r *Run) Path() string { return r.path }

// Finish rewrites metadata.json with a terminal status and end_time.
func (r *Run) Finish(status string) error {
	switch status {
	case RunStatusCompleted, RunStatusFailed:
	default:
		return fmt.Errorf("invalid terminal run status %q", status)
	}
	return r.writeMetadata(status, time.Now().UTC())
}

func (r *Run) writeMetadata(status string, end time.Time) error {
	metadata := RunMetadata{
		SchemaVersion: runMetadataSchemaVersion,
		RunID:         r.id,
		StartTime:     r.start.Format(time.RFC3339Nano),
		Args:          r.args,
		Status:        status,
	}
	if !end.IsZero() {
		metadata.EndTime = end.Format(time.RFC3339Nano)
	}
	if metadata.Args == nil {
		metadata.Args = []string{}
	}
	bytes, err := json.MarshalIndent(metadata, "", "  ")
	if err != nil {
		return fmt.Errorf("marshal run metadata: %w", err)
	}
	path := filepath.Join(r.path, metadataFileName)
	if err := os.WriteFile(path, append(bytes, '\n'), 0o644); err != nil {
		return fmt.Errorf("write run metadata: %w", err)
	}
	return nil
}

// ListRuns returns one RunInfo per direct subdirectory of runs/, sorted by
// StartTime ascending (oldest first). Entries with missing or corrupt
// metadata.json fall back to the directory modification time and carry
// HasMetadata=false; the caller (gc) can still process them.
func (w *Workspace) ListRuns() ([]RunInfo, error) {
	entries, err := os.ReadDir(w.RunsDir())
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, fmt.Errorf("read runs dir: %w", err)
	}
	runs := make([]RunInfo, 0, len(entries))
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}
		info := readRunInfo(w.RunsDir(), entry.Name())
		runs = append(runs, info)
	}
	sort.Slice(runs, func(i, j int) bool {
		return runs[i].StartTime.Before(runs[j].StartTime)
	})
	return runs, nil
}

func readRunInfo(runsDir, id string) RunInfo {
	path := filepath.Join(runsDir, id)
	info := RunInfo{RunID: id, Path: path}

	metadataPath := filepath.Join(path, metadataFileName)
	if bytes, err := os.ReadFile(metadataPath); err == nil {
		var metadata RunMetadata
		if err := json.Unmarshal(bytes, &metadata); err == nil {
			if start, err := time.Parse(time.RFC3339Nano, metadata.StartTime); err == nil {
				info.StartTime = start
				info.HasMetadata = true
				info.Metadata = metadata
			}
		}
	}
	if !info.HasMetadata {
		if stat, err := os.Stat(path); err == nil {
			info.StartTime = stat.ModTime()
		}
	}
	if size, err := dirSize(path); err == nil {
		info.SizeBytes = size
	}
	return info
}

func generateRunID(start time.Time) (string, error) {
	timestamp := start.UTC().Format("20060102T150405Z")
	suffix := make([]byte, runIDSuffixBytes)
	if _, err := rand.Read(suffix); err != nil {
		return "", fmt.Errorf("generate run id suffix: %w", err)
	}
	return timestamp + "-" + hex.EncodeToString(suffix), nil
}
