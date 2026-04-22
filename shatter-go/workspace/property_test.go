package workspace

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"pgregory.net/rapid"
)

// TestGCInvariantsProperty uses rapid to verify that PlanGC and RunGC uphold
// their core invariants under arbitrary inputs:
//   - PlanGC never deletes files (filesystem is unchanged).
//   - DryRun=true never deletes files.
//   - Candidates returned by PlanGC always have a recognised reason code.
//   - BytesPlanned equals the sum of individual candidate sizes.
func TestGCInvariantsProperty(outer *testing.T) {
	rapid.Check(outer, func(t *rapid.T) {
		ws := openWorkspace(outer)

		// Generate between 0 and 15 runs with random start times and sizes.
		runCount := rapid.IntRange(0, 15).Draw(t, "runCount")
		base := time.Date(2026, 1, 1, 0, 0, 0, 0, time.UTC)
		for i := range runCount {
			offsetHours := rapid.Int64Range(0, 8760).Draw(t, "offsetHours")
			start := base.Add(time.Duration(offsetHours) * time.Hour)
			sizeBytes := rapid.IntRange(0, 512*1024).Draw(t, "sizeBytes")
			seedRunWithSize(outer, ws, i%26, start, RunStatusCompleted, sizeBytes)
		}

		keepN := rapid.IntRange(-1, 25).Draw(t, "keepN")
		maxAgeDays := rapid.IntRange(-1, 365).Draw(t, "maxAgeDays")
		maxRunsMiB := rapid.IntRange(-1, 10).Draw(t, "maxRunsMiB")

		opts := GCOptions{
			KeepLastN:     keepN,
			MaxAge:        time.Duration(maxAgeDays) * 24 * time.Hour,
			MaxRunsBytes:  int64(maxRunsMiB) * 1024 * 1024,
			MaxCacheBytes: -1,
			Now:           base.Add(400 * 24 * time.Hour),
		}

		// PlanGC must not modify the filesystem.
		beforeEntries := countRunDirs(outer, ws)
		report, err := ws.PlanGC(opts)
		if err != nil {
			t.Fatal(err)
		}
		afterEntries := countRunDirs(outer, ws)
		if beforeEntries != afterEntries {
			t.Fatalf("PlanGC modified the filesystem: had %d dirs, now %d", beforeEntries, afterEntries)
		}

		// All candidates must have a recognised reason.
		validReasons := map[string]struct{}{
			GCReasonCount: {}, GCReasonAge: {}, GCReasonSize: {}, GCReasonCacheSize: {},
		}
		for _, candidate := range report.Candidates {
			if _, ok := validReasons[candidate.Reason]; !ok {
				t.Fatalf("candidate has unrecognised reason %q", candidate.Reason)
			}
		}

		// BytesPlanned must equal sum of candidate sizes.
		var totalCandidateBytes int64
		for _, candidate := range report.Candidates {
			totalCandidateBytes += candidate.Size
		}
		if report.BytesPlanned != totalCandidateBytes {
			t.Fatalf("BytesPlanned %d != sum of candidate sizes %d", report.BytesPlanned, totalCandidateBytes)
		}
	})
}

// TestRunGCSizeCappedProperty verifies that after RunGC with a size cap, the
// surviving runs/ directory is at or below the cap.
func TestRunGCSizeCappedProperty(outer *testing.T) {
	rapid.Check(outer, func(t *rapid.T) {
		ws := openWorkspace(outer)

		runCount := rapid.IntRange(1, 12).Draw(t, "runCount")
		base := time.Date(2026, 1, 1, 0, 0, 0, 0, time.UTC)
		for i := range runCount {
			offsetHours := rapid.Int64Range(0, 2000).Draw(t, "offsetHours")
			start := base.Add(time.Duration(offsetHours) * time.Hour)
			sizeBytes := rapid.IntRange(1024, 256*1024).Draw(t, "sizeBytes")
			seedRunWithSize(outer, ws, i%26, start, RunStatusCompleted, sizeBytes)
		}

		// Cap is between 1 byte and 2 MiB. Zero is reserved as the "use default"
		// sentinel in GCOptions, so we start at 1 to stay in the explicit-value
		// range and keep the test deterministic.
		capBytes := int64(rapid.IntRange(1, 2*1024*1024).Draw(t, "capBytes"))

		_, err := ws.RunGC(GCOptions{
			KeepLastN:     -1,
			MaxAge:        -1,
			MaxRunsBytes:  capBytes,
			MaxCacheBytes: -1,
			Now:           base.Add(400 * 24 * time.Hour),
		})
		if err != nil {
			t.Fatal(err)
		}

		// Count surviving size.
		var survivingBytes int64
		entries, err := os.ReadDir(ws.RunsDir())
		if err != nil && !os.IsNotExist(err) {
			t.Fatal(err)
		}
		for _, entry := range entries {
			size, err := dirSize(filepath.Join(ws.RunsDir(), entry.Name()))
			if err != nil {
				t.Fatal(err)
			}
			survivingBytes += size
		}

		if survivingBytes > capBytes {
			t.Fatalf("surviving runs/ size %d > cap %d after RunGC", survivingBytes, capBytes)
		}
	})
}

// TestDryRunNeverDeletesProperty verifies that DryRun=true never removes files
// regardless of which rules are active.
func TestDryRunNeverDeletesProperty(outer *testing.T) {
	rapid.Check(outer, func(t *rapid.T) {
		ws := openWorkspace(outer)

		runCount := rapid.IntRange(0, 20).Draw(t, "runCount")
		base := time.Date(2026, 1, 1, 0, 0, 0, 0, time.UTC)
		for i := range runCount {
			offsetHours := rapid.Int64Range(0, 8760).Draw(t, "offsetHours")
			start := base.Add(time.Duration(offsetHours) * time.Hour)
			seedRun(outer, ws, i%26, start, RunStatusCompleted)
		}

		keepN := rapid.IntRange(0, 30).Draw(t, "keepN")
		opts := GCOptions{
			KeepLastN:     keepN,
			MaxAge:        14 * 24 * time.Hour,
			MaxRunsBytes:  1024 * 1024, // 1 MiB — tight cap
			MaxCacheBytes: -1,
			DryRun:        true,
			Now:           base.Add(400 * 24 * time.Hour),
		}

		beforeEntries := countRunDirs(outer, ws)
		report, err := ws.RunGC(opts)
		if err != nil {
			t.Fatal(err)
		}
		afterEntries := countRunDirs(outer, ws)

		if beforeEntries != afterEntries {
			t.Fatalf("DryRun=true deleted files: had %d dirs, now %d", beforeEntries, afterEntries)
		}
		if len(report.Deleted) != 0 {
			t.Fatalf("DryRun=true produced non-empty Deleted: %v", report.Deleted)
		}
	})
}

// TestNewRunIDUniqueProperty verifies that rapidly-created run IDs are unique.
func TestNewRunIDUniqueProperty(outer *testing.T) {
	rapid.Check(outer, func(t *rapid.T) {
		ws := openWorkspace(outer)
		count := rapid.IntRange(2, 8).Draw(t, "count")
		seen := make(map[string]struct{}, count)
		for range count {
			run, err := ws.NewRun([]string{"prop-test"})
			if err != nil {
				t.Fatal(err)
			}
			if _, dup := seen[run.ID()]; dup {
				t.Fatalf("duplicate run ID: %s", run.ID())
			}
			seen[run.ID()] = struct{}{}
		}
	})
}

// countRunDirs returns the number of direct subdirectory entries in ws.RunsDir().
func countRunDirs(t *testing.T, ws *Workspace) int {
	t.Helper()
	entries, err := os.ReadDir(ws.RunsDir())
	if os.IsNotExist(err) {
		return 0
	}
	if err != nil {
		t.Fatalf("countRunDirs: %v", err)
	}
	count := 0
	for _, entry := range entries {
		if entry.IsDir() {
			count++
		}
	}
	return count
}
