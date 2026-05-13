package workspace

import (
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"time"
)

const (
	DefaultGCKeepLastN         = 20
	DefaultGCMaxAge            = 14 * 24 * time.Hour
	DefaultGCMaxRunsBytes      = int64(5) * 1024 * 1024 * 1024 // 5 GiB
	DefaultGCMaxCacheBytes     = int64(5) * 1024 * 1024 * 1024 // 5 GiB
	DefaultGCMaxGeneratedBytes = int64(1) * 1024 * 1024 * 1024 // 1 GiB
	DefaultGCMaxBinariesBytes  = int64(1) * 1024 * 1024 * 1024 // 1 GiB

	GCReasonCount         = "count"
	GCReasonAge           = "age"
	GCReasonSize          = "size"
	GCReasonCacheSize     = "cache-size"
	GCReasonGeneratedAge  = "generated-age"
	GCReasonGeneratedSize = "generated-size"
	GCReasonBinariesAge   = "binaries-age"
	GCReasonBinariesSize  = "binaries-size"
)

// GCOptions controls which runs and cache entries are eligible for eviction.
// Zero values receive the DefaultGC* sentinels below; set a bound explicitly
// to a negative number to disable the corresponding rule.
type GCOptions struct {
	KeepLastN         int
	MaxAge            time.Duration
	MaxRunsBytes      int64
	MaxCacheBytes     int64
	MaxGeneratedBytes int64
	MaxBinariesBytes  int64
	DryRun            bool

	// Now is injectable for tests so age decisions stay deterministic.
	// Defaults to time.Now().
	Now time.Time
}

// GCCandidate is a single path gc proposes to remove.
type GCCandidate struct {
	Path   string
	Reason string
	Size   int64
	RunID  string // empty for cache files
}

// CacheSizes records the pre-gc and post-gc total bytes for each cache dir.
type CacheSizes struct {
	Before int64
	After  int64
}

// GCReport summarizes a planning or execution pass.
type GCReport struct {
	Scanned             int
	Candidates          []GCCandidate
	Deleted             []string
	BytesPlanned        int64
	BytesRemoved        int64
	RunsSizeBefore      int64
	RunsSizeAfter       int64
	CacheSizes          map[string]CacheSizes
	GeneratedSizeBefore int64
	GeneratedSizeAfter  int64
	BinariesSizeBefore  int64
	BinariesSizeAfter   int64
}

// PlanGC enumerates runs and caches and returns the set of paths that would
// be removed by the current options. No filesystem mutations occur. Pass
// DryRun=false to RunGC to actually delete.
func (w *Workspace) PlanGC(opts GCOptions) (*GCReport, error) {
	return w.planOrRun(opts, false)
}

// RunGC plans and, unless opts.DryRun is true, deletes the planned paths.
func (w *Workspace) RunGC(opts GCOptions) (*GCReport, error) {
	return w.planOrRun(opts, !opts.DryRun)
}

func (w *Workspace) planOrRun(opts GCOptions, apply bool) (*GCReport, error) {
	opts = withGCDefaults(opts)

	runs, err := w.ListRuns()
	if err != nil {
		return nil, err
	}
	runsSizeBefore := sumRunSizes(runs)

	candidates, err := planRunCandidates(runs, opts)
	if err != nil {
		return nil, err
	}

	cacheDirs := map[string]string{
		"cache/build":  w.BuildCacheDir(),
		"cache/loader": w.LoaderCacheDir(),
	}
	cacheSizes := make(map[string]CacheSizes, len(cacheDirs))
	cacheDirNames := make([]string, 0, len(cacheDirs))
	for name := range cacheDirs {
		cacheDirNames = append(cacheDirNames, name)
	}
	sort.Strings(cacheDirNames)
	for _, name := range cacheDirNames {
		before, cacheCandidates, err := planCacheCandidates(cacheDirs[name], opts.MaxCacheBytes)
		if err != nil {
			return nil, err
		}
		cacheSizes[name] = CacheSizes{Before: before, After: before}
		candidates = append(candidates, cacheCandidates...)
	}

	generatedBefore, generatedCandidates, err := planHashedDirCandidates(w.GeneratedDir(), opts.MaxGeneratedBytes, opts.MaxAge, opts.Now, GCReasonGeneratedAge, GCReasonGeneratedSize)
	if err != nil {
		return nil, err
	}
	candidates = append(candidates, generatedCandidates...)

	binariesBefore, binariesCandidates, err := planFileTreeCandidates(w.BinariesDir(), opts.MaxBinariesBytes, opts.MaxAge, opts.Now, GCReasonBinariesAge, GCReasonBinariesSize, registryFileName)
	if err != nil {
		return nil, err
	}
	candidates = append(candidates, binariesCandidates...)

	report := &GCReport{
		Scanned:             len(runs),
		Candidates:          candidates,
		RunsSizeBefore:      runsSizeBefore,
		RunsSizeAfter:       runsSizeBefore,
		CacheSizes:          cacheSizes,
		GeneratedSizeBefore: generatedBefore,
		GeneratedSizeAfter:  generatedBefore,
		BinariesSizeBefore:  binariesBefore,
		BinariesSizeAfter:   binariesBefore,
	}
	for _, candidate := range candidates {
		report.BytesPlanned += candidate.Size
	}

	if !apply {
		return report, nil
	}

	for _, candidate := range candidates {
		if err := os.RemoveAll(candidate.Path); err != nil {
			return report, fmt.Errorf("remove %s: %w", candidate.Path, err)
		}
		report.Deleted = append(report.Deleted, candidate.Path)
		report.BytesRemoved += candidate.Size
	}

	report.RunsSizeAfter = runsSizeBefore - bytesFromCandidates(candidates, GCReasonCount, GCReasonAge, GCReasonSize)
	for name, sizes := range cacheSizes {
		removed := bytesFromCandidatesInPath(candidates, cacheDirs[name])
		sizes.After = sizes.Before - removed
		cacheSizes[name] = sizes
	}
	report.GeneratedSizeAfter = generatedBefore - bytesFromCandidates(candidates, GCReasonGeneratedAge, GCReasonGeneratedSize)
	report.BinariesSizeAfter = binariesBefore - bytesFromCandidates(candidates, GCReasonBinariesAge, GCReasonBinariesSize)
	return report, nil
}

// registryFileName names the build registry index that lives alongside
// compiled launcher binaries. It is excluded from binaries/ eviction
// because BinaryRegistry.Lookup self-evicts entries whose binary is
// missing, so leaving the index intact is harmless and avoids needing
// callers to recreate it.
const registryFileName = "binary_registry.json"

func withGCDefaults(opts GCOptions) GCOptions {
	if opts.KeepLastN == 0 {
		opts.KeepLastN = DefaultGCKeepLastN
	}
	if opts.MaxAge == 0 {
		opts.MaxAge = DefaultGCMaxAge
	}
	if opts.MaxRunsBytes == 0 {
		opts.MaxRunsBytes = DefaultGCMaxRunsBytes
	}
	if opts.MaxCacheBytes == 0 {
		opts.MaxCacheBytes = DefaultGCMaxCacheBytes
	}
	if opts.MaxGeneratedBytes == 0 {
		opts.MaxGeneratedBytes = DefaultGCMaxGeneratedBytes
	}
	if opts.MaxBinariesBytes == 0 {
		opts.MaxBinariesBytes = DefaultGCMaxBinariesBytes
	}
	if opts.Now.IsZero() {
		opts.Now = time.Now()
	}
	return opts
}

func planRunCandidates(runs []RunInfo, opts GCOptions) ([]GCCandidate, error) {
	// runs is sorted ascending (oldest first). Build newest-first view.
	newestFirst := make([]RunInfo, len(runs))
	for i, r := range runs {
		newestFirst[len(runs)-1-i] = r
	}

	evicted := make(map[string]GCCandidate)
	ageCutoff := opts.Now.Add(-opts.MaxAge)

	// Rule: keep only the N newest.
	if opts.KeepLastN >= 0 {
		for index := opts.KeepLastN; index < len(newestFirst); index++ {
			run := newestFirst[index]
			evicted[run.Path] = GCCandidate{
				Path: run.Path, Reason: GCReasonCount, Size: run.SizeBytes, RunID: run.RunID,
			}
		}
	}

	// Rule: drop anything older than cutoff.
	if opts.MaxAge > 0 {
		for _, run := range runs {
			if run.StartTime.Before(ageCutoff) {
				if _, exists := evicted[run.Path]; !exists {
					evicted[run.Path] = GCCandidate{
						Path: run.Path, Reason: GCReasonAge, Size: run.SizeBytes, RunID: run.RunID,
					}
				}
			}
		}
	}

	// Rule: cap total runs/ size. Evict oldest survivors first until under cap.
	if opts.MaxRunsBytes >= 0 {
		survivors := make([]RunInfo, 0, len(runs))
		total := int64(0)
		for _, run := range runs {
			if _, evictedAlready := evicted[run.Path]; evictedAlready {
				continue
			}
			survivors = append(survivors, run)
			total += run.SizeBytes
		}
		// survivors is oldest-first (inherited from runs).
		for total > opts.MaxRunsBytes && len(survivors) > 0 {
			oldest := survivors[0]
			survivors = survivors[1:]
			evicted[oldest.Path] = GCCandidate{
				Path: oldest.Path, Reason: GCReasonSize, Size: oldest.SizeBytes, RunID: oldest.RunID,
			}
			total -= oldest.SizeBytes
		}
	}

	candidates := make([]GCCandidate, 0, len(evicted))
	for _, candidate := range evicted {
		candidates = append(candidates, candidate)
	}
	sort.Slice(candidates, func(i, j int) bool {
		return candidates[i].Path < candidates[j].Path
	})
	return candidates, nil
}

type cacheFile struct {
	path    string
	size    int64
	modTime time.Time
}

func planCacheCandidates(cacheDir string, maxBytes int64) (totalBefore int64, candidates []GCCandidate, err error) {
	files, err := collectCacheFiles(cacheDir)
	if err != nil {
		return 0, nil, err
	}
	for _, file := range files {
		totalBefore += file.size
	}
	if maxBytes < 0 || totalBefore <= maxBytes {
		return totalBefore, nil, nil
	}

	// Oldest-first.
	sort.Slice(files, func(i, j int) bool {
		return files[i].modTime.Before(files[j].modTime)
	})
	remaining := totalBefore
	for _, file := range files {
		if remaining <= maxBytes {
			break
		}
		candidates = append(candidates, GCCandidate{
			Path: file.path, Reason: GCReasonCacheSize, Size: file.size,
		})
		remaining -= file.size
	}
	return totalBefore, candidates, nil
}

func collectCacheFiles(cacheDir string) ([]cacheFile, error) {
	var files []cacheFile
	err := filepath.WalkDir(cacheDir, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			if os.IsNotExist(err) {
				return fs.SkipDir
			}
			return err
		}
		if d.IsDir() {
			return nil
		}
		info, err := d.Info()
		if err != nil {
			return err
		}
		files = append(files, cacheFile{
			path:    path,
			size:    info.Size(),
			modTime: info.ModTime(),
		})
		return nil
	})
	if err != nil {
		if os.IsNotExist(err) {
			return nil, nil
		}
		return nil, fmt.Errorf("walk cache dir %s: %w", cacheDir, err)
	}
	return files, nil
}

func dirSize(root string) (int64, error) {
	var total int64
	err := filepath.WalkDir(root, func(_ string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.IsDir() {
			return nil
		}
		info, err := d.Info()
		if err != nil {
			return err
		}
		total += info.Size()
		return nil
	})
	if err != nil {
		if os.IsNotExist(err) {
			return 0, nil
		}
		return 0, err
	}
	return total, nil
}

func sumRunSizes(runs []RunInfo) int64 {
	var total int64
	for _, run := range runs {
		total += run.SizeBytes
	}
	return total
}

func bytesFromCandidates(candidates []GCCandidate, reasons ...string) int64 {
	allowed := make(map[string]struct{}, len(reasons))
	for _, reason := range reasons {
		allowed[reason] = struct{}{}
	}
	var total int64
	for _, candidate := range candidates {
		if _, ok := allowed[candidate.Reason]; ok {
			total += candidate.Size
		}
	}
	return total
}

func bytesFromCandidatesInPath(candidates []GCCandidate, pathPrefix string) int64 {
	var total int64
	for _, candidate := range candidates {
		if candidate.Reason != GCReasonCacheSize {
			continue
		}
		if hasPathPrefix(candidate.Path, pathPrefix) {
			total += candidate.Size
		}
	}
	return total
}

// planHashedDirCandidates evicts direct subdirectories of root by age then
// total size. Used for generated/, which is keyed by per-target discovery
// hash and not reachable via any in-process index — stale entries are pure
// dead weight on disk. Entries older than maxAge are always evicted; the
// remainder is capped at maxBytes by removing the oldest survivors first.
// A negative cap disables that rule.
func planHashedDirCandidates(root string, maxBytes int64, maxAge time.Duration, now time.Time, ageReason, sizeReason string) (totalBefore int64, candidates []GCCandidate, err error) {
	entries, err := os.ReadDir(root)
	if err != nil {
		if os.IsNotExist(err) {
			return 0, nil, nil
		}
		return 0, nil, fmt.Errorf("read dir %s: %w", root, err)
	}

	type hashedEntry struct {
		path    string
		size    int64
		modTime time.Time
	}
	collected := make([]hashedEntry, 0, len(entries))
	for _, entry := range entries {
		if !entry.IsDir() {
			continue
		}
		path := filepath.Join(root, entry.Name())
		info, statErr := os.Stat(path)
		if statErr != nil {
			continue
		}
		size, sizeErr := dirSize(path)
		if sizeErr != nil {
			return 0, nil, fmt.Errorf("size %s: %w", path, sizeErr)
		}
		collected = append(collected, hashedEntry{path: path, size: size, modTime: info.ModTime()})
		totalBefore += size
	}

	evicted := make(map[string]struct{})
	if maxAge > 0 {
		ageCutoff := now.Add(-maxAge)
		for _, entry := range collected {
			if entry.modTime.Before(ageCutoff) {
				candidates = append(candidates, GCCandidate{Path: entry.path, Reason: ageReason, Size: entry.size})
				evicted[entry.path] = struct{}{}
			}
		}
	}

	if maxBytes >= 0 {
		survivors := make([]hashedEntry, 0, len(collected))
		surviving := int64(0)
		for _, entry := range collected {
			if _, ok := evicted[entry.path]; ok {
				continue
			}
			survivors = append(survivors, entry)
			surviving += entry.size
		}
		sort.Slice(survivors, func(i, j int) bool {
			return survivors[i].modTime.Before(survivors[j].modTime)
		})
		for surviving > maxBytes && len(survivors) > 0 {
			oldest := survivors[0]
			survivors = survivors[1:]
			candidates = append(candidates, GCCandidate{Path: oldest.path, Reason: sizeReason, Size: oldest.size})
			surviving -= oldest.size
		}
	}
	return totalBefore, candidates, nil
}

// planFileTreeCandidates evicts files under root by age then total size.
// Used for binaries/, which is a flat tree of compiled launcher binaries
// keyed by discovery hash. Filenames in skipNames are never evicted —
// these are persistent index files (e.g. binary_registry.json) maintained
// alongside the binaries.
func planFileTreeCandidates(root string, maxBytes int64, maxAge time.Duration, now time.Time, ageReason, sizeReason string, skipNames ...string) (totalBefore int64, candidates []GCCandidate, err error) {
	skip := make(map[string]struct{}, len(skipNames))
	for _, name := range skipNames {
		skip[name] = struct{}{}
	}

	files, err := collectCacheFiles(root)
	if err != nil {
		return 0, nil, err
	}
	filtered := files[:0]
	for _, file := range files {
		if _, isSkipped := skip[filepath.Base(file.path)]; isSkipped {
			continue
		}
		filtered = append(filtered, file)
		totalBefore += file.size
	}

	evicted := make(map[string]struct{})
	if maxAge > 0 {
		ageCutoff := now.Add(-maxAge)
		for _, file := range filtered {
			if file.modTime.Before(ageCutoff) {
				candidates = append(candidates, GCCandidate{Path: file.path, Reason: ageReason, Size: file.size})
				evicted[file.path] = struct{}{}
			}
		}
	}

	if maxBytes >= 0 {
		survivors := make([]cacheFile, 0, len(filtered))
		surviving := int64(0)
		for _, file := range filtered {
			if _, ok := evicted[file.path]; ok {
				continue
			}
			survivors = append(survivors, file)
			surviving += file.size
		}
		sort.Slice(survivors, func(i, j int) bool {
			return survivors[i].modTime.Before(survivors[j].modTime)
		})
		for surviving > maxBytes && len(survivors) > 0 {
			oldest := survivors[0]
			survivors = survivors[1:]
			candidates = append(candidates, GCCandidate{Path: oldest.path, Reason: sizeReason, Size: oldest.size})
			surviving -= oldest.size
		}
	}
	return totalBefore, candidates, nil
}

func hasPathPrefix(path, prefix string) bool {
	if len(path) < len(prefix) {
		return false
	}
	if path[:len(prefix)] != prefix {
		return false
	}
	if len(path) == len(prefix) {
		return true
	}
	return path[len(prefix)] == filepath.Separator
}
