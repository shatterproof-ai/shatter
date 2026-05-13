package workspace

import (
	"fmt"
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

// seedGeneratedHashDir creates a fake generated/<hash> subtree with
// controlled mtime and contents totalling sizeBytes. Used to exercise
// the str-5zjc generated/ pruning path.
func seedGeneratedHashDir(t *testing.T, workspace *Workspace, hash string, mtime time.Time, sizeBytes int) string {
	t.Helper()
	dir := filepath.Join(workspace.GeneratedDir(), hash)
	if err := os.MkdirAll(filepath.Join(dir, "launcher"), 0o755); err != nil {
		t.Fatalf("mkdir generated/%s: %v", hash, err)
	}
	if sizeBytes > 0 {
		if err := os.WriteFile(filepath.Join(dir, "launcher", "main.go"), make([]byte, sizeBytes), 0o644); err != nil {
			t.Fatalf("write generated payload: %v", err)
		}
	}
	if err := os.Chtimes(dir, mtime, mtime); err != nil {
		t.Fatalf("chtimes generated dir: %v", err)
	}
	return dir
}

// seedBinary creates a fake binaries/shatter_launcher_<hash> file with
// controlled mtime and size. Used to exercise the str-5zjc binaries/
// pruning path.
func seedBinary(t *testing.T, workspace *Workspace, hash string, mtime time.Time, sizeBytes int) string {
	t.Helper()
	path := filepath.Join(workspace.BinariesDir(), "shatter_launcher_"+hash)
	if err := os.WriteFile(path, make([]byte, sizeBytes), 0o755); err != nil {
		t.Fatalf("write binary: %v", err)
	}
	if err := os.Chtimes(path, mtime, mtime); err != nil {
		t.Fatalf("chtimes binary: %v", err)
	}
	return path
}

// TestPlanGCBoundsGeneratedDir verifies that the str-5zjc workspace disk
// bound is applied to generated/<hash> entries: anything older than MaxAge
// is evicted by age, and the oldest survivors are evicted by size until
// total generated/ size is within MaxGeneratedBytes.
func TestPlanGCBoundsGeneratedDir(t *testing.T) {
	workspace := openWorkspace(t)
	now := time.Date(2026, 5, 13, 0, 0, 0, 0, time.UTC)
	const perEntryBytes = 512 * 1024 // 512 KiB

	// Three old entries → should be age-evicted.
	for index := 0; index < 3; index++ {
		seedGeneratedHashDir(t, workspace, fmt.Sprintf("old%d", index), now.Add(-30*24*time.Hour), perEntryBytes)
	}
	// Five fresh entries; cap allows only two by size.
	for index := 0; index < 5; index++ {
		seedGeneratedHashDir(t, workspace, fmt.Sprintf("fresh%d", index), now.Add(time.Duration(index)*time.Hour), perEntryBytes)
	}

	const cap = int64(2 * perEntryBytes)
	report, err := workspace.PlanGC(GCOptions{
		KeepLastN:         -1,
		MaxAge:            14 * 24 * time.Hour,
		MaxRunsBytes:      -1,
		MaxCacheBytes:     -1,
		MaxGeneratedBytes: cap,
		MaxBinariesBytes:  -1,
		Now:               now,
	})
	if err != nil {
		t.Fatalf("PlanGC: %v", err)
	}

	ageEvictions := 0
	sizeEvictions := 0
	for _, candidate := range report.Candidates {
		switch candidate.Reason {
		case GCReasonGeneratedAge:
			ageEvictions++
		case GCReasonGeneratedSize:
			sizeEvictions++
		}
	}
	if ageEvictions != 3 {
		t.Errorf("got %d age evictions, want 3", ageEvictions)
	}
	if sizeEvictions != 3 {
		t.Errorf("got %d size evictions (5 fresh - cap of 2), want 3", sizeEvictions)
	}
	// Surviving size must be at or under the cap.
	surviving := report.GeneratedSizeBefore
	for _, candidate := range report.Candidates {
		if candidate.Reason == GCReasonGeneratedAge || candidate.Reason == GCReasonGeneratedSize {
			surviving -= candidate.Size
		}
	}
	if surviving > cap {
		t.Errorf("surviving generated/ size %d > cap %d", surviving, cap)
	}
}

// TestRunGCPrunesBinariesPreservingRegistry verifies the str-5zjc binaries/
// pruning path: stale launcher binaries are deleted, total binaries/ size
// falls under the cap, and the persistent binary_registry.json index is
// never evicted (BinaryRegistry self-evicts stale lookups, so leaving the
// index alone is harmless).
func TestRunGCPrunesBinariesPreservingRegistry(t *testing.T) {
	workspace := openWorkspace(t)
	now := time.Date(2026, 5, 13, 0, 0, 0, 0, time.UTC)
	const perBinaryBytes = 1024 * 1024 // 1 MiB

	for index := 0; index < 6; index++ {
		seedBinary(t, workspace, fmt.Sprintf("hash%02d", index), now.Add(time.Duration(index)*time.Minute), perBinaryBytes)
	}
	registryPath := filepath.Join(workspace.BinariesDir(), "binary_registry.json")
	if err := os.WriteFile(registryPath, []byte(`{}`), 0o644); err != nil {
		t.Fatalf("write registry: %v", err)
	}

	const cap = int64(2 * perBinaryBytes)
	report, err := workspace.RunGC(GCOptions{
		KeepLastN:         -1,
		MaxAge:            -1,
		MaxRunsBytes:      -1,
		MaxCacheBytes:     -1,
		MaxGeneratedBytes: -1,
		MaxBinariesBytes:  cap,
		Now:               now,
	})
	if err != nil {
		t.Fatalf("RunGC: %v", err)
	}

	binariesEvicted := 0
	for _, candidate := range report.Candidates {
		if candidate.Reason == GCReasonBinariesSize {
			binariesEvicted++
		}
		if filepath.Base(candidate.Path) == "binary_registry.json" {
			t.Errorf("registry was selected for eviction at %q; it must be preserved", candidate.Path)
		}
	}
	if binariesEvicted != 4 {
		t.Errorf("got %d binaries evicted (6 - cap of 2), want 4", binariesEvicted)
	}
	if _, err := os.Stat(registryPath); err != nil {
		t.Errorf("registry must remain on disk after gc: %v", err)
	}
	if report.BinariesSizeAfter > cap {
		t.Errorf("BinariesSizeAfter %d > cap %d", report.BinariesSizeAfter, cap)
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
