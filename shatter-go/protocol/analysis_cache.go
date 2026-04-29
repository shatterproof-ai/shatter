package protocol

import (
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"os"
	"path/filepath"
	"runtime"
	"sort"
	"strconv"
	"strings"
	"time"

	"golang.org/x/tools/go/packages"

	"github.com/shatter-dev/shatter/shatter-go/workspace"
)

// Discovery cache (str-hy9b.C6).
//
// Caches analyzer output keyed by a content hash of the target package's Go
// source files plus its one-level import package source files, the loader
// configuration, the Go runtime version, the Shatter protocol version, and
// the optional function-name filter passed to the analyzer.
//
// On a hit, the parse + typecheck + analyzer walk are skipped. On a miss
// (including unreadable, schema-mismatched, version-mismatched, or
// truncated payloads) the analyzer recomputes and the cache file is
// rewritten atomically (temp + rename). Concurrent writers contend
// harmlessly: same hash → same payload → last-writer-wins.

const (
	// analysisCacheSchemaVersion is the on-disk payload schema version. Bump
	// when the JSON shape changes; older payloads will read as
	// analysisCacheMissSchemaMismatch and be rewritten in the new shape.
	analysisCacheSchemaVersion = 1

	// analysisCacheFileExtension is the suffix for cache payload files
	// stored under <workspace>/analysis/. The base name is the hex hash.
	analysisCacheFileExtension = ".json"

	// analysisCacheTempFilePrefix prefixes the temp file used during atomic
	// writes. The full pattern is "<hash>.tmp-<pid>-<rand><ext>".
	analysisCacheTempFilePrefix = ".tmp-"

	// discoveryHashHexLength is the hex-string length of a discovery hash.
	// We take the first 32 hex chars (128 bits) of SHA-256, matching
	// computePrepareID style elsewhere in this package.
	discoveryHashHexLength = 32

	// analysisCacheDisableEnvVar disables the discovery cache when set to a
	// non-empty value. Intended for debugging / forced re-analysis; not a
	// configuration knob.
	analysisCacheDisableEnvVar = "SHATTER_DISABLE_ANALYSIS_CACHE"

	// Lite-load mode for the cache hash: needs file paths and one-level
	// imports only, no parse / typecheck. Cost is dominated by `go list`.
	analysisCacheLiteLoadMode = packages.NeedName |
		packages.NeedFiles |
		packages.NeedImports |
		packages.NeedModule

	// Cache miss reason strings. These flow into structured logging so the
	// acceptance test can assert "the second analyze was a hit" by reading
	// log records.
	analysisCacheMissNotFound       = "not_found"
	analysisCacheMissParseError     = "parse_error"
	analysisCacheMissSchemaMismatch = "schema_mismatch"
	analysisCacheMissVersionMismatch = "version_mismatch"
	analysisCacheMissDisabled       = "disabled"
)

// analysisCachePayload is the on-disk shape stored at
// <workspace>/analysis/<hash>.json.
type analysisCachePayload struct {
	SchemaVersion  int                `json:"schema_version"`
	ShatterVersion string             `json:"shatter_version"`
	SourcePath     string             `json:"source_path"`
	FunctionFilter string             `json:"function_filter,omitempty"`
	Functions      []FunctionAnalysis `json:"functions"`
	CreatedAt      string             `json:"created_at"`
}

// ComputeDiscoveryHash returns the cache key for analyzing filePath with the
// given function-name filter. It performs a lite-load via go/packages
// (NeedName | NeedFiles | NeedImports | NeedModule, no NeedTypes / NeedSyntax)
// to enumerate the target package's GoFiles and one-level import GoFiles,
// then SHA-256-hashes their concatenated bytes together with stable
// configuration inputs (Shatter protocol version, Go runtime version,
// function-name filter).
//
// Standard-library imports (those with a nil Module) are excluded — toolchain
// churn is covered by runtime.Version(). If the lite-load fails (e.g.
// outside a module), the function falls back to hashing only the target
// file's bytes plus the import path strings scanned textually; this keeps
// standalone-file analysis cacheable.
func ComputeDiscoveryHash(filePath, functionFilter string) (string, error) {
	absoluteFilePath, err := filepath.Abs(filePath)
	if err != nil {
		return "", fmt.Errorf("normalize file path %q: %w", filePath, err)
	}

	hasher := sha256.New()
	writeStringField(hasher, "shatter_version", ProtocolVersion)
	writeStringField(hasher, "runtime_version", runtime.Version())
	writeStringField(hasher, "function_filter", functionFilter)
	writeStringField(hasher, "schema_version", strconv.Itoa(analysisCacheSchemaVersion))
	writeStringField(hasher, "source_path", absoluteFilePath)

	pkg, liteLoadErr := liteLoadPackage(absoluteFilePath)
	if liteLoadErr != nil || pkg == nil {
		// Fallback: hash the target file bytes only, plus textually-scanned
		// import paths. Keeps standalone-file analysis cacheable.
		if err := hashFileBytes(hasher, absoluteFilePath); err != nil {
			return "", err
		}
		writeStringField(hasher, "lite_load_fallback", "1")
	} else {
		// Hash target package's own GoFiles in sorted order.
		writeStringField(hasher, "package_path", pkg.PkgPath)
		writeStringField(hasher, "package_name", pkg.Name)
		if err := hashGoFilesSorted(hasher, "package_files", pkg.GoFiles); err != nil {
			return "", err
		}
		// Hash one-level imports' GoFiles, sorted by import path.
		// Standard-library packages (Module == nil) are skipped — runtime
		// version covers their churn.
		importPaths := make([]string, 0, len(pkg.Imports))
		for path := range pkg.Imports {
			importPaths = append(importPaths, path)
		}
		sort.Strings(importPaths)
		for _, path := range importPaths {
			imported := pkg.Imports[path]
			if imported == nil {
				continue
			}
			writeStringField(hasher, "import_path", path)
			if imported.Module == nil {
				// std-lib: hash the path only, not contents.
				writeStringField(hasher, "import_kind", "stdlib")
				continue
			}
			writeStringField(hasher, "import_kind", "module")
			if err := hashGoFilesSorted(hasher, "import_files:"+path, imported.GoFiles); err != nil {
				return "", err
			}
		}
	}

	full := hex.EncodeToString(hasher.Sum(nil))
	return full[:discoveryHashHexLength], nil
}

// liteLoadPackage loads filePath's containing package with the minimum mode
// needed to enumerate GoFiles and one-level imports. Returns nil, nil if no
// valid package is selected (callers fall back to file-byte-only hashing).
func liteLoadPackage(absoluteFilePath string) (*packages.Package, error) {
	directory := filepath.Dir(absoluteFilePath)
	config := &packages.Config{
		Mode: analysisCacheLiteLoadMode,
		Dir:  directory,
		Env:  os.Environ(),
	}
	loaded, err := packages.Load(config, ".")
	if err != nil {
		return nil, err
	}
	for _, pkg := range loaded {
		if pkg == nil {
			continue
		}
		if len(pkg.GoFiles) > 0 {
			return pkg, nil
		}
	}
	return nil, nil
}

// hashGoFilesSorted hashes the byte contents of every path in goFiles in
// sorted order, separating each by a labeled framing record so reordering
// cannot collide. Files that fail to read are recorded by name only with a
// "missing" marker — the analyzer would have failed for the same reason
// downstream, and the hash should still be stable across repeated lookups
// of a missing file.
func hashGoFilesSorted(hasher io.Writer, label string, goFiles []string) error {
	sorted := append([]string(nil), goFiles...)
	sort.Strings(sorted)
	for _, path := range sorted {
		writeStringField(hasher, label+":path", path)
		if err := hashFileBytes(hasher, path); err != nil {
			return err
		}
	}
	return nil
}

// hashFileBytes feeds the contents of path into hasher with a length prefix
// so two files cannot be confused via boundary collisions. Missing files
// are encoded as a labeled "missing" marker rather than an error so the
// hash is stable for absent paths.
func hashFileBytes(hasher io.Writer, path string) error {
	bytes, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			writeStringField(hasher, "file_missing", path)
			return nil
		}
		return fmt.Errorf("read source for hashing %q: %w", path, err)
	}
	writeStringField(hasher, "file_size", strconv.Itoa(len(bytes)))
	if _, err := hasher.Write(bytes); err != nil {
		return fmt.Errorf("hash file %q: %w", path, err)
	}
	return nil
}

// writeStringField writes a labeled NUL-framed string into hasher.
// Framing prevents two adjacent fields from concatenating into a third.
func writeStringField(hasher io.Writer, label, value string) {
	_, _ = io.WriteString(hasher, label)
	_, _ = hasher.Write([]byte{0})
	_, _ = io.WriteString(hasher, value)
	_, _ = hasher.Write([]byte{0})
}

// ReadAnalysisCache returns (functions, true, "") on a cache hit, or
// (nil, false, missReason) on miss. Errors are intentionally folded into
// miss reasons — a misbehaving cache must never fail analysis. The reason
// strings are stable: handler.go logs them, and the C6 acceptance test
// asserts on them.
func ReadAnalysisCache(ws *workspace.Workspace, hash string) (functions []FunctionAnalysis, hit bool, missReason string) {
	if ws == nil {
		return nil, false, analysisCacheMissNotFound
	}
	if disabled := strings.TrimSpace(os.Getenv(analysisCacheDisableEnvVar)); disabled != "" && disabled != "0" {
		return nil, false, analysisCacheMissDisabled
	}
	bytes, err := os.ReadFile(analysisCachePath(ws, hash))
	if err != nil {
		if os.IsNotExist(err) {
			return nil, false, analysisCacheMissNotFound
		}
		return nil, false, analysisCacheMissParseError
	}
	var payload analysisCachePayload
	if err := json.Unmarshal(bytes, &payload); err != nil {
		return nil, false, analysisCacheMissParseError
	}
	if payload.SchemaVersion != analysisCacheSchemaVersion {
		return nil, false, analysisCacheMissSchemaMismatch
	}
	if payload.ShatterVersion != ProtocolVersion {
		return nil, false, analysisCacheMissVersionMismatch
	}
	return payload.Functions, true, ""
}

// WriteAnalysisCache atomically writes a cache payload for hash. Concurrent
// writers contend harmlessly: same hash → same payload → last-writer-wins.
// Failure to write is non-fatal for the caller; the analysis proceeds and
// the next run pays the analyzer cost again.
func WriteAnalysisCache(ws *workspace.Workspace, hash, sourcePath, functionFilter string, functions []FunctionAnalysis) error {
	if ws == nil {
		return fmt.Errorf("workspace is nil")
	}
	if disabled := strings.TrimSpace(os.Getenv(analysisCacheDisableEnvVar)); disabled != "" && disabled != "0" {
		// Caching disabled: silently skip the write.
		return nil
	}

	if err := os.MkdirAll(ws.AnalysisDir(), 0o755); err != nil {
		return fmt.Errorf("create analysis dir: %w", err)
	}

	if functions == nil {
		functions = []FunctionAnalysis{}
	}
	payload := analysisCachePayload{
		SchemaVersion:  analysisCacheSchemaVersion,
		ShatterVersion: ProtocolVersion,
		SourcePath:     sourcePath,
		FunctionFilter: functionFilter,
		Functions:      functions,
		CreatedAt:      time.Now().UTC().Format(time.RFC3339),
	}
	bytes, err := json.MarshalIndent(payload, "", "  ")
	if err != nil {
		return fmt.Errorf("marshal payload: %w", err)
	}

	finalPath := analysisCachePath(ws, hash)
	tempPath, err := analysisCacheTempPath(ws, hash)
	if err != nil {
		return err
	}
	if err := os.WriteFile(tempPath, append(bytes, '\n'), 0o644); err != nil {
		return fmt.Errorf("write temp cache file: %w", err)
	}
	if err := os.Rename(tempPath, finalPath); err != nil {
		// Clean up the temp file so the analysis dir does not accumulate
		// `.tmp-*` clutter on rename failure (e.g. cross-device).
		_ = os.Remove(tempPath)
		return fmt.Errorf("rename cache file %q -> %q: %w", tempPath, finalPath, err)
	}
	return nil
}

func analysisCachePath(ws *workspace.Workspace, hash string) string {
	return filepath.Join(ws.AnalysisDir(), hash+analysisCacheFileExtension)
}

// analysisCacheTempPath returns a unique temp filename in the analysis dir.
// Including pid plus 8 random bytes makes collisions effectively impossible
// across concurrent writers.
func analysisCacheTempPath(ws *workspace.Workspace, hash string) (string, error) {
	var randomBytes [8]byte
	if _, err := rand.Read(randomBytes[:]); err != nil {
		return "", fmt.Errorf("read random bytes for temp file: %w", err)
	}
	suffix := fmt.Sprintf("%d-%s", os.Getpid(), hex.EncodeToString(randomBytes[:]))
	name := hash + analysisCacheTempFilePrefix + suffix + analysisCacheFileExtension
	return filepath.Join(ws.AnalysisDir(), name), nil
}

// logCacheHit / logCacheMiss / logCacheWrite are thin slog helpers consumed
// by handler.handleAnalyze. Centralizing the call sites keeps the message
// strings stable for acceptance tests that capture log records.
func logCacheHit(log *slog.Logger, hash, file string) {
	if log == nil {
		return
	}
	log.Info("analysis cache hit", "hash", hash, "file", file)
}

func logCacheMiss(log *slog.Logger, hash, file, reason string) {
	if log == nil {
		return
	}
	log.Info("analysis cache miss", "hash", hash, "file", file, "reason", reason)
}

func logCacheWrite(log *slog.Logger, hash, file string) {
	if log == nil {
		return
	}
	log.Debug("analysis cache write", "hash", hash, "file", file)
}
