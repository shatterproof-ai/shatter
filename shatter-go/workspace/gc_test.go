package workspace

import (
	"os"
	"path/filepath"
	"testing"
	"time"
)

func TestPlanGCKeepLastN(t *testing.T) {
	workspace := openWorkspace(t)
	base := time.Date(2026, 4, 1, 0, 0, 0, 0, time.UTC)
	for index := 0; index < 25; index++ {
		seedRun(t, workspace, index, base.Add(time.Duration(index)*time.Hour), RunStatusCompleted)
	}

	report, err := workspace.PlanGC(GCOptions{
		KeepLastN:     20,
		MaxAge:        -1,
		MaxRunsBytes:  -1,
		MaxCacheBytes: -1,
		Now:           base.Add(30 * time.Hour),
	})
	if err != nil {
		t.Fatalf("PlanGC: %v", err)
	}
	if len(report.Candidates) != 5 {
		t.Fatalf("got %d candidates, want 5", len(report.Candidates))
	}
	for _, candidate := range report.Candidates {
		if candidate.Reason != GCReasonCount {
			t.Fatalf("reason = %q, want %q", candidate.Reason, GCReasonCount)
		}
	}
}

func TestPlanGCMaxAge(t *testing.T) {
	workspace := openWorkspace(t)
	now := time.Date(2026, 4, 18, 0, 0, 0, 0, time.UTC)
	seedRun(t, workspace, 0, now.Add(-30*24*time.Hour), RunStatusCompleted) // too old
	seedRun(t, workspace, 1, now.Add(-20*24*time.Hour), RunStatusCompleted) // too old
	seedRun(t, workspace, 2, now.Add(-5*24*time.Hour), RunStatusCompleted)  // kept

	report, err := workspace.PlanGC(GCOptions{
		KeepLastN:     -1,
		MaxAge:        14 * 24 * time.Hour,
		MaxRunsBytes:  -1,
		MaxCacheBytes: -1,
		Now:           now,
	})
	if err != nil {
		t.Fatalf("PlanGC: %v", err)
	}
	if len(report.Candidates) != 2 {
		t.Fatalf("got %d candidates, want 2", len(report.Candidates))
	}
	for _, candidate := range report.Candidates {
		if candidate.Reason != GCReasonAge {
			t.Fatalf("reason = %q, want %q", candidate.Reason, GCReasonAge)
		}
	}
}

func TestPlanGCMaxRunsBytes(t *testing.T) {
	workspace := openWorkspace(t)
	base := time.Date(2026, 4, 1, 0, 0, 0, 0, time.UTC)
	const perRunBytes = 1024 * 1024 // 1 MiB each
	for index := 0; index < 5; index++ {
		seedRunWithSize(t, workspace, index, base.Add(time.Duration(index)*time.Hour), RunStatusCompleted, perRunBytes)
	}

	const cap = int64(2*perRunBytes) + 4096 // +slack for metadata.json
	report, err := workspace.PlanGC(GCOptions{
		KeepLastN:     -1,
		MaxAge:        -1,
		MaxRunsBytes:  cap,
		MaxCacheBytes: -1,
		Now:           base.Add(10 * time.Hour),
	})
	if err != nil {
		t.Fatalf("PlanGC: %v", err)
	}
	if len(report.Candidates) != 3 {
		t.Fatalf("got %d size candidates, want 3", len(report.Candidates))
	}
	survivingSize := report.RunsSizeBefore
	for _, candidate := range report.Candidates {
		if candidate.Reason != GCReasonSize {
			t.Fatalf("reason = %q, want %q", candidate.Reason, GCReasonSize)
		}
		survivingSize -= candidate.Size
	}
	if survivingSize > cap {
		t.Fatalf("surviving runs/ size %d > cap %d", survivingSize, cap)
	}
}

func TestPlanGCDryRunLeavesFiles(t *testing.T) {
	workspace := openWorkspace(t)
	base := time.Date(2026, 4, 1, 0, 0, 0, 0, time.UTC)
	for index := 0; index < 25; index++ {
		seedRun(t, workspace, index, base.Add(time.Duration(index)*time.Hour), RunStatusCompleted)
	}

	report, err := workspace.RunGC(GCOptions{
		KeepLastN: 20,
		DryRun:    true,
		Now:       base.Add(30 * time.Hour),
	})
	if err != nil {
		t.Fatalf("RunGC: %v", err)
	}
	if len(report.Deleted) != 0 {
		t.Fatalf("DryRun Deleted = %v, want empty", report.Deleted)
	}
	for _, candidate := range report.Candidates {
		if _, err := os.Stat(candidate.Path); err != nil {
			t.Fatalf("candidate %q should still exist after dry-run: %v", candidate.Path, err)
		}
	}
}

func TestRunGCActuallyDeletesAndEnforcesCap(t *testing.T) {
	workspace := openWorkspace(t)
	base := time.Date(2026, 4, 1, 0, 0, 0, 0, time.UTC)
	const perRunBytes = 1024 * 1024 // 1 MiB
	for index := 0; index < 10; index++ {
		seedRunWithSize(t, workspace, index, base.Add(time.Duration(index)*time.Hour), RunStatusCompleted, perRunBytes)
	}

	const cap = int64(3 * perRunBytes)
	report, err := workspace.RunGC(GCOptions{
		KeepLastN:     -1,
		MaxAge:        -1,
		MaxRunsBytes:  cap,
		MaxCacheBytes: -1,
		Now:           base.Add(20 * time.Hour),
	})
	if err != nil {
		t.Fatalf("RunGC: %v", err)
	}
	if len(report.Deleted) == 0 {
		t.Fatal("expected some deletions to enforce cap")
	}

	var total int64
	entries, err := os.ReadDir(workspace.RunsDir())
	if err != nil {
		t.Fatalf("read runs: %v", err)
	}
	for _, entry := range entries {
		size, err := dirSize(filepath.Join(workspace.RunsDir(), entry.Name()))
		if err != nil {
			t.Fatalf("dirSize: %v", err)
		}
		total += size
	}
	if total > cap {
		t.Fatalf("total runs/ size %d > cap %d after RunGC", total, cap)
	}
}

func TestPlanGCCacheSizeCap(t *testing.T) {
	workspace := openWorkspace(t)
	buildDir := workspace.BuildCacheDir()
	for index := 0; index < 5; index++ {
		filename := filepath.Join(buildDir, filepath.Join("pkg", "file"+string(rune('a'+index))+".bin"))
		if err := os.MkdirAll(filepath.Dir(filename), 0o755); err != nil {
			t.Fatalf("mkdir cache: %v", err)
		}
		if err := os.WriteFile(filename, make([]byte, 1024*1024), 0o644); err != nil {
			t.Fatalf("write cache file: %v", err)
		}
		// Back-date so oldest files get evicted first.
		backdated := time.Date(2026, 1, 1+index, 0, 0, 0, 0, time.UTC)
		if err := os.Chtimes(filename, backdated, backdated); err != nil {
			t.Fatalf("chtimes: %v", err)
		}
	}

	report, err := workspace.PlanGC(GCOptions{
		KeepLastN:     -1,
		MaxAge:        -1,
		MaxRunsBytes:  -1,
		MaxCacheBytes: 2 * 1024 * 1024,
	})
	if err != nil {
		t.Fatalf("PlanGC: %v", err)
	}
	cacheCandidates := 0
	for _, candidate := range report.Candidates {
		if candidate.Reason == GCReasonCacheSize {
			cacheCandidates++
		}
	}
	if cacheCandidates != 3 {
		t.Fatalf("got %d cache candidates, want 3", cacheCandidates)
	}
}

func TestPlanGCMissingMetadataFallsBackToMtime(t *testing.T) {
	workspace := openWorkspace(t)

	bareDir := filepath.Join(workspace.RunsDir(), "legacy-run")
	if err := os.MkdirAll(bareDir, 0o755); err != nil {
		t.Fatalf("mkdir legacy: %v", err)
	}
	oldTime := time.Date(2025, 1, 1, 0, 0, 0, 0, time.UTC)
	if err := os.Chtimes(bareDir, oldTime, oldTime); err != nil {
		t.Fatalf("chtimes legacy: %v", err)
	}

	report, err := workspace.PlanGC(GCOptions{
		KeepLastN:     -1,
		MaxAge:        30 * 24 * time.Hour,
		MaxRunsBytes:  -1,
		MaxCacheBytes: -1,
		Now:           time.Date(2026, 4, 18, 0, 0, 0, 0, time.UTC),
	})
	if err != nil {
		t.Fatalf("PlanGC: %v", err)
	}
	if len(report.Candidates) != 1 {
		t.Fatalf("got %d candidates, want 1", len(report.Candidates))
	}
	if report.Candidates[0].Reason != GCReasonAge {
		t.Fatalf("reason = %q, want %q", report.Candidates[0].Reason, GCReasonAge)
	}
}

func TestPlanGCNowInjectableDeterminism(t *testing.T) {
	workspace := openWorkspace(t)
	now := time.Date(2026, 4, 18, 0, 0, 0, 0, time.UTC)
	seedRun(t, workspace, 0, now.Add(-14*24*time.Hour-time.Hour), RunStatusCompleted) // just past cutoff
	seedRun(t, workspace, 1, now.Add(-14*24*time.Hour+time.Hour), RunStatusCompleted) // just under

	report, err := workspace.PlanGC(GCOptions{
		KeepLastN:     -1,
		MaxAge:        14 * 24 * time.Hour,
		MaxRunsBytes:  -1,
		MaxCacheBytes: -1,
		Now:           now,
	})
	if err != nil {
		t.Fatalf("PlanGC: %v", err)
	}
	if len(report.Candidates) != 1 {
		t.Fatalf("got %d candidates, want 1", len(report.Candidates))
	}
}
