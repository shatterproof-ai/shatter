package workspace

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"
)

const (
	EnvironmentRootKey = "SHATTER_GO_WORKSPACE_ROOT"

	applicationDirectoryName = "shatter"
	workspaceDirectoryName   = "go-workspace"
	repositoryCacheDirectory = ".shatter-cache"

	analysisDirectoryName  = "analysis"
	plansDirectoryName     = "plans"
	generatedDirectoryName = "generated"
	overlaysDirectoryName  = "overlays"
	binariesDirectoryName  = "binaries"
	runsDirectoryName      = "runs"
	cacheDirectoryName     = "cache"
	buildDirectoryName     = "build"
	loaderDirectoryName    = "loader"
	metadataFileName       = "metadata.json"

	metadataSchemaVersion = 1
	workspaceToolName     = "shatter-go"

	repositoryLayoutDocPath = "docs/PROJECT-LAYOUT.md"
	repositoryModuleDirName = "shatter-go"
	goModuleFileName        = "go.mod"
	mainFileName            = "main.go"
)

// ResolveOptions controls workspace root resolution.
type ResolveOptions struct {
	StartDir         string
	RepoOverrideRoot string
}

// Workspace owns the Go frontend's artifact workspace tree.
type Workspace struct {
	root string
}

type metadata struct {
	SchemaVersion int    `json:"schema_version"`
	Tool          string `json:"tool"`
	Root          string `json:"root"`
	CreatedAt     string `json:"created_at"`
}

// Open returns a workspace handle for an already chosen root path.
func Open(root string) (*Workspace, error) {
	absoluteRoot, err := absolutePath(root)
	if err != nil {
		return nil, fmt.Errorf("normalize workspace root: %w", err)
	}
	return &Workspace{root: absoluteRoot}, nil
}

// Initialize resolves the workspace root, ensures the directory tree exists,
// and writes metadata.json on first initialization.
func Initialize(options ResolveOptions) (*Workspace, error) {
	root, err := ResolveRoot(options)
	if err != nil {
		return nil, err
	}

	workspace, err := Open(root)
	if err != nil {
		return nil, err
	}
	if err := workspace.Ensure(); err != nil {
		return nil, err
	}
	return workspace, nil
}

// ResolveRoot chooses the workspace root using the required precedence:
// environment variable, repository override, then user-data default.
func ResolveRoot(options ResolveOptions) (string, error) {
	if environmentRoot := strings.TrimSpace(os.Getenv(EnvironmentRootKey)); environmentRoot != "" {
		absoluteRoot, err := absolutePath(environmentRoot)
		if err != nil {
			return "", fmt.Errorf("resolve workspace root from %s: %w", EnvironmentRootKey, err)
		}
		return absoluteRoot, nil
	}

	if repoOverrideRoot := strings.TrimSpace(options.RepoOverrideRoot); repoOverrideRoot != "" {
		absoluteRoot, err := absolutePath(repoOverrideRoot)
		if err != nil {
			return "", fmt.Errorf("resolve repository workspace root: %w", err)
		}
		return absoluteRoot, nil
	}

	repositoryRoot, found, err := findRepositoryRoot(options.StartDir)
	if err != nil {
		return "", err
	}
	if found {
		return filepath.Join(repositoryRoot, repositoryCacheDirectory, workspaceDirectoryName), nil
	}

	root, err := userDataDefaultRoot()
	if err != nil {
		return "", err
	}
	return root, nil
}

// Ensure creates the directory tree and writes metadata.json if it does not
// already exist.
func (w *Workspace) Ensure() error {
	for _, directory := range []string{
		w.Root(),
		w.AnalysisDir(),
		w.PlansDir(),
		w.GeneratedDir(),
		w.OverlaysDir(),
		w.BinariesDir(),
		w.RunsDir(),
		w.CacheDir(),
		w.BuildCacheDir(),
		w.LoaderCacheDir(),
	} {
		if err := os.MkdirAll(directory, 0o755); err != nil {
			return fmt.Errorf("create workspace directory %q: %w", directory, err)
		}
	}

	if err := w.ensureMetadata(); err != nil {
		return err
	}
	return nil
}

// Root returns the workspace root directory.
func (w *Workspace) Root() string {
	return w.root
}

// AnalysisDir returns the analysis artifact directory.
func (w *Workspace) AnalysisDir() string {
	return filepath.Join(w.root, analysisDirectoryName)
}

// PlansDir returns the plans directory.
func (w *Workspace) PlansDir() string {
	return filepath.Join(w.root, plansDirectoryName)
}

// GeneratedDir returns the generated files directory.
func (w *Workspace) GeneratedDir() string {
	return filepath.Join(w.root, generatedDirectoryName)
}

// OverlaysDir returns the overlays directory.
func (w *Workspace) OverlaysDir() string {
	return filepath.Join(w.root, overlaysDirectoryName)
}

// BinariesDir returns the binary output directory.
func (w *Workspace) BinariesDir() string {
	return filepath.Join(w.root, binariesDirectoryName)
}

// RunsDir returns the per-run state directory.
func (w *Workspace) RunsDir() string {
	return filepath.Join(w.root, runsDirectoryName)
}

// CacheDir returns the cache root.
func (w *Workspace) CacheDir() string {
	return filepath.Join(w.root, cacheDirectoryName)
}

// BuildCacheDir returns the build cache directory.
func (w *Workspace) BuildCacheDir() string {
	return filepath.Join(w.CacheDir(), buildDirectoryName)
}

// LoaderCacheDir returns the loader cache directory.
func (w *Workspace) LoaderCacheDir() string {
	return filepath.Join(w.CacheDir(), loaderDirectoryName)
}

// MetadataPath returns the metadata.json path.
func (w *Workspace) MetadataPath() string {
	return filepath.Join(w.root, metadataFileName)
}

func (w *Workspace) ensureMetadata() error {
	if _, err := os.Stat(w.MetadataPath()); err == nil {
		return nil
	} else if !os.IsNotExist(err) {
		return fmt.Errorf("stat workspace metadata: %w", err)
	}

	data, err := json.MarshalIndent(metadata{
		SchemaVersion: metadataSchemaVersion,
		Tool:          workspaceToolName,
		Root:          w.root,
		CreatedAt:     time.Now().UTC().Format(time.RFC3339),
	}, "", "  ")
	if err != nil {
		return fmt.Errorf("marshal workspace metadata: %w", err)
	}

	if err := os.WriteFile(w.MetadataPath(), append(data, '\n'), 0o644); err != nil {
		return fmt.Errorf("write workspace metadata: %w", err)
	}
	return nil
}

func userDataDefaultRoot() (string, error) {
	configDir, err := os.UserConfigDir()
	if err != nil {
		return "", fmt.Errorf("resolve user config dir: %w", err)
	}
	return filepath.Join(configDir, applicationDirectoryName, workspaceDirectoryName), nil
}

func absolutePath(path string) (string, error) {
	absolutePath, err := filepath.Abs(path)
	if err != nil {
		return "", err
	}
	return absolutePath, nil
}

func findRepositoryRoot(startDir string) (string, bool, error) {
	searchDir := startDir
	if strings.TrimSpace(searchDir) == "" {
		currentDir, err := os.Getwd()
		if err != nil {
			return "", false, fmt.Errorf("determine current working directory: %w", err)
		}
		searchDir = currentDir
	}

	absoluteStartDir, err := absolutePath(searchDir)
	if err != nil {
		return "", false, fmt.Errorf("normalize repository search dir: %w", err)
	}

	for candidate := absoluteStartDir; ; candidate = filepath.Dir(candidate) {
		if isRepositoryRoot(candidate) {
			if filepath.Base(candidate) == repositoryModuleDirName {
				return filepath.Dir(candidate), true, nil
			}
			return candidate, true, nil
		}

		parent := filepath.Dir(candidate)
		if parent == candidate {
			return "", false, nil
		}
	}
}

func isRepositoryRoot(candidate string) bool {
	moduleDir := filepath.Join(candidate, repositoryModuleDirName)
	if fileExists(filepath.Join(candidate, repositoryLayoutDocPath)) &&
		fileExists(filepath.Join(moduleDir, goModuleFileName)) &&
		fileExists(filepath.Join(moduleDir, mainFileName)) {
		return true
	}

	parent := filepath.Dir(candidate)
	if parent == candidate {
		return false
	}

	return fileExists(filepath.Join(parent, repositoryLayoutDocPath)) &&
		fileExists(filepath.Join(candidate, goModuleFileName)) &&
		fileExists(filepath.Join(candidate, mainFileName)) &&
		filepath.Base(candidate) == repositoryModuleDirName
}

func fileExists(path string) bool {
	info, err := os.Stat(path)
	return err == nil && !info.IsDir()
}
