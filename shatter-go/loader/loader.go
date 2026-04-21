package loader

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/shatter-dev/shatter/shatter-go/workspace"
	"golang.org/x/tools/go/packages"
)

const (
	cacheEntrySchemaVersion = 1

	cacheEntriesDirectoryName    = "entries"
	cacheMaterializedDirectory   = "materialized"
	cacheEntryFileExtension      = ".json"
	cacheKindPackage             = "package"
	cacheKindFile                = "file"
	syntheticModulePathPrefix    = "shatter.synthetic/"
	syntheticModuleGoVersion     = "1.23.0"
	syntheticModuleFilePerms     = 0o644
	syntheticModuleDirectoryPerm = 0o755
)

const packageLoadMode = packages.NeedName |
	packages.NeedFiles |
	packages.NeedImports |
	packages.NeedDeps |
	packages.NeedTypes |
	packages.NeedSyntax |
	packages.NeedTypesInfo |
	packages.NeedModule

// Loader resolves Go packages through go/packages and stores loader metadata
// inside the workspace cache.
type Loader struct {
	workspace    *workspace.Workspace
	packageByKey map[string]*packages.Package
}

type cacheEntry struct {
	SchemaVersion    int    `json:"schema_version"`
	Kind             string `json:"kind"`
	SourcePath       string `json:"source_path"`
	LoadDir          string `json:"load_dir"`
	MaterializedRoot string `json:"materialized_root,omitempty"`
	MaterializedFile string `json:"materialized_file,omitempty"`
	SourceHash       string `json:"source_hash,omitempty"`
}

// New constructs a loader that stores its cache state under the provided
// workspace.
func New(workspace *workspace.Workspace) (*Loader, error) {
	if workspace == nil {
		return nil, fmt.Errorf("loader requires a workspace")
	}
	if err := workspace.Ensure(); err != nil {
		return nil, fmt.Errorf("ensure workspace: %w", err)
	}
	return &Loader{
		workspace:    workspace,
		packageByKey: make(map[string]*packages.Package),
	}, nil
}

// LoadPackage loads the package rooted at packageDirectory.
func (l *Loader) LoadPackage(packageDirectory string) (*packages.Package, error) {
	return l.loadPackage(packageDirectory, strictValidation)
}

// LoadPackageLenient is like LoadPackage but tolerates go/packages Errors
// (e.g. unresolved imports). Syntax, Types, and TypesInfo are still required
// to be populated. Intended for the analyzer, whose predecessor swallowed
// typecheck errors to keep partial type info. Strict callers must use
// LoadPackage.
func (l *Loader) LoadPackageLenient(packageDirectory string) (*packages.Package, error) {
	return l.loadPackage(packageDirectory, lenientValidation)
}

func (l *Loader) loadPackage(packageDirectory string, validation validationMode) (*packages.Package, error) {
	absoluteDirectory, err := filepath.Abs(packageDirectory)
	if err != nil {
		return nil, fmt.Errorf("normalize package directory %q: %w", packageDirectory, err)
	}

	directoryInfo, err := os.Stat(absoluteDirectory)
	if err != nil {
		return nil, fmt.Errorf("stat package directory %q: %w", absoluteDirectory, err)
	}
	if !directoryInfo.IsDir() {
		return nil, fmt.Errorf("package directory %q is not a directory", absoluteDirectory)
	}

	cacheKey := cacheKeyFor(cacheKindPackage, absoluteDirectory)
	entry, err := l.readOrCreatePackageEntry(cacheKey, absoluteDirectory)
	if err != nil {
		return nil, err
	}
	return l.loadFromEntry(cacheKey, entry, validation)
}

// LoadFile loads a standalone Go file by materializing a synthetic module in
// the workspace loader cache.
func (l *Loader) LoadFile(filePath string) (*packages.Package, error) {
	return l.loadFile(filePath, strictValidation)
}

// LoadFileLenient is like LoadFile but tolerates go/packages Errors. See
// LoadPackageLenient for the rationale.
func (l *Loader) LoadFileLenient(filePath string) (*packages.Package, error) {
	return l.loadFile(filePath, lenientValidation)
}

func (l *Loader) loadFile(filePath string, validation validationMode) (*packages.Package, error) {
	absoluteFilePath, err := filepath.Abs(filePath)
	if err != nil {
		return nil, fmt.Errorf("normalize file path %q: %w", filePath, err)
	}

	sourceBytes, err := os.ReadFile(absoluteFilePath)
	if err != nil {
		return nil, fmt.Errorf("read source file %q: %w", absoluteFilePath, err)
	}

	sourceInfo, err := os.Stat(absoluteFilePath)
	if err != nil {
		return nil, fmt.Errorf("stat source file %q: %w", absoluteFilePath, err)
	}
	if sourceInfo.IsDir() {
		return nil, fmt.Errorf("source path %q is a directory", absoluteFilePath)
	}

	cacheKey := cacheKeyFor(cacheKindFile, absoluteFilePath)
	entry, err := l.readOrCreateFileEntry(cacheKey, absoluteFilePath)
	if err != nil {
		return nil, err
	}

	sourceHash := hashBytes(sourceBytes)
	if entry.SourceHash != sourceHash {
		if err := l.materializeStandaloneFile(entry, absoluteFilePath, sourceBytes, cacheKey); err != nil {
			return nil, err
		}
		entry.SourceHash = sourceHash
		if err := l.writeCacheEntry(cacheKey, entry); err != nil {
			return nil, err
		}
	}

	return l.loadFromEntry(cacheKey, entry, validation)
}

func (l *Loader) readOrCreatePackageEntry(cacheKey string, absoluteDirectory string) (*cacheEntry, error) {
	entry, found, err := l.readCacheEntry(cacheKey)
	if err != nil {
		return nil, err
	}
	if found {
		return entry, nil
	}

	entry = &cacheEntry{
		SchemaVersion: cacheEntrySchemaVersion,
		Kind:          cacheKindPackage,
		SourcePath:    absoluteDirectory,
		LoadDir:       absoluteDirectory,
	}
	if err := l.writeCacheEntry(cacheKey, entry); err != nil {
		return nil, err
	}
	return entry, nil
}

func (l *Loader) readOrCreateFileEntry(cacheKey string, absoluteFilePath string) (*cacheEntry, error) {
	entry, found, err := l.readCacheEntry(cacheKey)
	if err != nil {
		return nil, err
	}
	if found {
		return entry, nil
	}

	materializedRoot := filepath.Join(l.materializedRootDir(), cacheKey)
	entry = &cacheEntry{
		SchemaVersion:    cacheEntrySchemaVersion,
		Kind:             cacheKindFile,
		SourcePath:       absoluteFilePath,
		LoadDir:          materializedRoot,
		MaterializedRoot: materializedRoot,
		MaterializedFile: filepath.Join(materializedRoot, filepath.Base(absoluteFilePath)),
	}
	if err := l.writeCacheEntry(cacheKey, entry); err != nil {
		return nil, err
	}
	return entry, nil
}

func (l *Loader) materializeStandaloneFile(entry *cacheEntry, absoluteFilePath string, sourceBytes []byte, cacheKey string) error {
	if err := os.MkdirAll(entry.MaterializedRoot, syntheticModuleDirectoryPerm); err != nil {
		return fmt.Errorf("create synthetic module root %q: %w", entry.MaterializedRoot, err)
	}

	goModPath := filepath.Join(entry.MaterializedRoot, "go.mod")
	goModContents := fmt.Sprintf(
		"module %s%s\n\ngo %s\n",
		syntheticModulePathPrefix,
		cacheKey,
		syntheticModuleGoVersion,
	)
	if err := os.WriteFile(goModPath, []byte(goModContents), syntheticModuleFilePerms); err != nil {
		return fmt.Errorf("write synthetic go.mod %q: %w", goModPath, err)
	}

	materializedBytes := stripSyntheticBuildConstraints(sourceBytes)
	if err := os.WriteFile(entry.MaterializedFile, materializedBytes, syntheticModuleFilePerms); err != nil {
		return fmt.Errorf("write materialized source %q: %w", entry.MaterializedFile, err)
	}
	entry.LoadDir = entry.MaterializedRoot
	entry.SourcePath = absoluteFilePath
	return nil
}

// stripSyntheticBuildConstraints neutralizes leading `//go:build ...` and
// `// +build ...` constraint lines so the materialized copy is always
// picked up by go/packages. The synthetic module exists solely to host the
// file for analysis; honoring build tags that exclude the file would defeat
// its purpose. Each constraint line is replaced with a plain-comment line
// of the same line count so downstream byte offsets and line numbers stay
// aligned with the original source — callers that compare positions against
// the original file (e.g. AST-walking recognizers in test code) rely on
// this property.
func stripSyntheticBuildConstraints(source []byte) []byte {
	lines := strings.SplitAfter(string(source), "\n")
	var output strings.Builder
	inConstraintHeader := true
	for _, line := range lines {
		if inConstraintHeader {
			trimmed := strings.TrimSpace(line)
			if strings.HasPrefix(trimmed, "//go:build") || strings.HasPrefix(trimmed, "// +build") {
				if strings.HasSuffix(line, "\n") {
					output.WriteString("// shatter: build constraint stripped\n")
				} else {
					output.WriteString("// shatter: build constraint stripped")
				}
				continue
			}
			if trimmed == "" {
				output.WriteString(line)
				continue
			}
			if strings.HasPrefix(trimmed, "//") {
				output.WriteString(line)
				continue
			}
			inConstraintHeader = false
		}
		output.WriteString(line)
	}
	return []byte(output.String())
}

// validationMode selects the strictness of loaded-package validation.
type validationMode int

const (
	strictValidation validationMode = iota
	lenientValidation
)

func (l *Loader) loadFromEntry(cacheKey string, entry *cacheEntry, validation validationMode) (*packages.Package, error) {
	if loadedPackage, found := l.packageByKey[cacheKey]; found {
		return loadedPackage, nil
	}

	config := &packages.Config{
		Mode: packageLoadMode,
		Dir:  entry.LoadDir,
		Env:  os.Environ(),
	}

	loadedPackages, err := packages.Load(config, ".")
	if err != nil {
		return nil, fmt.Errorf("load package from %q: %w", entry.LoadDir, err)
	}
	loadedPackage, err := selectLoadedPackage(loadedPackages)
	if err != nil {
		return nil, fmt.Errorf("select loaded package for %q: %w", entry.SourcePath, err)
	}
	if err := validateLoadedPackage(loadedPackage, validation); err != nil {
		return nil, fmt.Errorf("validate loaded package for %q: %w", entry.SourcePath, err)
	}

	l.packageByKey[cacheKey] = loadedPackage
	return loadedPackage, nil
}

func selectLoadedPackage(loadedPackages []*packages.Package) (*packages.Package, error) {
	if len(loadedPackages) == 0 {
		return nil, fmt.Errorf("go/packages returned no packages")
	}

	for _, loadedPackage := range loadedPackages {
		if loadedPackage == nil {
			continue
		}
		if len(loadedPackage.GoFiles) > 0 || len(loadedPackage.CompiledGoFiles) > 0 {
			return loadedPackage, nil
		}
	}

	return nil, fmt.Errorf("go/packages returned only empty package results")
}

func validateLoadedPackage(loadedPackage *packages.Package, validation validationMode) error {
	if loadedPackage == nil {
		return fmt.Errorf("package is nil")
	}
	if validation == strictValidation && len(loadedPackage.Errors) > 0 {
		return fmt.Errorf("package load errors: %s", joinPackageErrors(loadedPackage.Errors))
	}
	if loadedPackage.Types == nil {
		return fmt.Errorf("package types are empty")
	}
	if len(loadedPackage.Syntax) == 0 {
		return fmt.Errorf("package syntax is empty")
	}
	if loadedPackage.TypesInfo == nil {
		return fmt.Errorf("package types info is empty")
	}
	return nil
}

func joinPackageErrors(packageErrors []packages.Error) string {
	messages := make([]string, 0, len(packageErrors))
	for _, packageError := range packageErrors {
		messages = append(messages, packageError.Error())
	}
	return strings.Join(messages, "; ")
}

func (l *Loader) readCacheEntry(cacheKey string) (*cacheEntry, bool, error) {
	entryBytes, err := os.ReadFile(l.cacheEntryPath(cacheKey))
	if err != nil {
		if os.IsNotExist(err) {
			return nil, false, nil
		}
		return nil, false, fmt.Errorf("read cache entry %q: %w", cacheKey, err)
	}

	var entry cacheEntry
	if err := json.Unmarshal(entryBytes, &entry); err != nil {
		return nil, false, fmt.Errorf("unmarshal cache entry %q: %w", cacheKey, err)
	}
	if entry.SchemaVersion != cacheEntrySchemaVersion {
		return nil, false, fmt.Errorf(
			"cache entry %q has schema version %d, want %d",
			cacheKey,
			entry.SchemaVersion,
			cacheEntrySchemaVersion,
		)
	}
	return &entry, true, nil
}

func (l *Loader) writeCacheEntry(cacheKey string, entry *cacheEntry) error {
	if err := os.MkdirAll(l.cacheEntriesDir(), syntheticModuleDirectoryPerm); err != nil {
		return fmt.Errorf("create loader cache entries dir: %w", err)
	}

	entryBytes, err := json.MarshalIndent(entry, "", "  ")
	if err != nil {
		return fmt.Errorf("marshal cache entry %q: %w", cacheKey, err)
	}
	if err := os.WriteFile(l.cacheEntryPath(cacheKey), append(entryBytes, '\n'), syntheticModuleFilePerms); err != nil {
		return fmt.Errorf("write cache entry %q: %w", cacheKey, err)
	}
	return nil
}

func (l *Loader) cacheEntryPath(cacheKey string) string {
	return filepath.Join(l.cacheEntriesDir(), cacheKey+cacheEntryFileExtension)
}

func (l *Loader) cacheEntriesDir() string {
	return filepath.Join(l.workspace.LoaderCacheDir(), cacheEntriesDirectoryName)
}

func (l *Loader) materializedRootDir() string {
	return filepath.Join(l.workspace.LoaderCacheDir(), cacheMaterializedDirectory)
}

func cacheKeyFor(kind string, sourcePath string) string {
	hashInput := kind + "\x00" + sourcePath
	sum := sha256.Sum256([]byte(hashInput))
	return hex.EncodeToString(sum[:])
}

func hashBytes(data []byte) string {
	sum := sha256.Sum256(data)
	return hex.EncodeToString(sum[:])
}
