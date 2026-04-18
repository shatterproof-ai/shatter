package workspace

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func TestResolveRootUsesEnvironmentVariable(t *testing.T) {
	t.Setenv(EnvironmentRootKey, filepath.Join(t.TempDir(), "env-root"))
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())

	repoRoot := makeRepositoryFixture(t)
	startDir := filepath.Join(repoRoot, repositoryModuleDirName, "protocol")
	if err := os.MkdirAll(startDir, 0o755); err != nil {
		t.Fatalf("mkdir startDir: %v", err)
	}

	root, err := ResolveRoot(ResolveOptions{StartDir: startDir})
	if err != nil {
		t.Fatalf("ResolveRoot: %v", err)
	}

	want, err := filepath.Abs(os.Getenv(EnvironmentRootKey))
	if err != nil {
		t.Fatalf("filepath.Abs(env): %v", err)
	}
	if root != want {
		t.Fatalf("ResolveRoot() = %q, want %q", root, want)
	}
}

func TestResolveRootUsesRepositoryOverrideWhenEnvironmentUnset(t *testing.T) {
	t.Setenv(EnvironmentRootKey, "")
	t.Setenv("XDG_CONFIG_HOME", t.TempDir())

	repoRoot := makeRepositoryFixture(t)
	startDir := filepath.Join(repoRoot, repositoryModuleDirName, "protocol")
	if err := os.MkdirAll(startDir, 0o755); err != nil {
		t.Fatalf("mkdir startDir: %v", err)
	}

	root, err := ResolveRoot(ResolveOptions{StartDir: startDir})
	if err != nil {
		t.Fatalf("ResolveRoot: %v", err)
	}

	want := filepath.Join(repoRoot, repositoryCacheDirectory, workspaceDirectoryName)
	if root != want {
		t.Fatalf("ResolveRoot() = %q, want %q", root, want)
	}
}

func TestResolveRootFallsBackToUserDataDefault(t *testing.T) {
	t.Setenv(EnvironmentRootKey, "")
	configHome := t.TempDir()
	t.Setenv("XDG_CONFIG_HOME", configHome)

	root, err := ResolveRoot(ResolveOptions{StartDir: t.TempDir()})
	if err != nil {
		t.Fatalf("ResolveRoot: %v", err)
	}

	want := filepath.Join(configHome, applicationDirectoryName, workspaceDirectoryName)
	if root != want {
		t.Fatalf("ResolveRoot() = %q, want %q", root, want)
	}
}

func TestWorkspaceEnsureCreatesDirectoriesAndMetadata(t *testing.T) {
	root := filepath.Join(t.TempDir(), "workspace-root")
	workspace, err := Open(root)
	if err != nil {
		t.Fatalf("Open: %v", err)
	}

	if err := workspace.Ensure(); err != nil {
		t.Fatalf("Ensure: %v", err)
	}

	for _, directory := range []string{
		workspace.AnalysisDir(),
		workspace.PlansDir(),
		workspace.GeneratedDir(),
		workspace.OverlaysDir(),
		workspace.BinariesDir(),
		workspace.CacheDir(),
		workspace.BuildCacheDir(),
		workspace.LoaderCacheDir(),
		workspace.RunsDir(),
	} {
		if info, err := os.Stat(directory); err != nil {
			t.Fatalf("stat %q: %v", directory, err)
		} else if !info.IsDir() {
			t.Fatalf("%q should be a directory", directory)
		}
	}

	metadataBytes, err := os.ReadFile(workspace.MetadataPath())
	if err != nil {
		t.Fatalf("ReadFile(metadata): %v", err)
	}

	var metadata metadata
	if err := json.Unmarshal(metadataBytes, &metadata); err != nil {
		t.Fatalf("Unmarshal(metadata): %v", err)
	}
	if metadata.SchemaVersion != metadataSchemaVersion {
		t.Fatalf("metadata schema_version = %d, want %d", metadata.SchemaVersion, metadataSchemaVersion)
	}
	if metadata.Tool != workspaceToolName {
		t.Fatalf("metadata tool = %q, want %q", metadata.Tool, workspaceToolName)
	}
	if metadata.Root != root {
		t.Fatalf("metadata root = %q, want %q", metadata.Root, root)
	}
	if metadata.CreatedAt == "" {
		t.Fatal("metadata created_at should not be empty")
	}
}

func TestWorkspaceEnsureIsIdempotent(t *testing.T) {
	root := filepath.Join(t.TempDir(), "workspace-root")
	workspace, err := Open(root)
	if err != nil {
		t.Fatalf("Open: %v", err)
	}

	if err := workspace.Ensure(); err != nil {
		t.Fatalf("Ensure(first): %v", err)
	}
	firstMetadata, err := os.ReadFile(workspace.MetadataPath())
	if err != nil {
		t.Fatalf("ReadFile(first metadata): %v", err)
	}

	if err := workspace.Ensure(); err != nil {
		t.Fatalf("Ensure(second): %v", err)
	}
	secondMetadata, err := os.ReadFile(workspace.MetadataPath())
	if err != nil {
		t.Fatalf("ReadFile(second metadata): %v", err)
	}

	if string(firstMetadata) != string(secondMetadata) {
		t.Fatal("metadata.json should be unchanged by a second Ensure() call")
	}
}

func makeRepositoryFixture(t *testing.T) string {
	t.Helper()

	repoRoot := t.TempDir()
	moduleDir := filepath.Join(repoRoot, repositoryModuleDirName)
	if err := os.MkdirAll(moduleDir, 0o755); err != nil {
		t.Fatalf("mkdir moduleDir: %v", err)
	}
	if err := os.MkdirAll(filepath.Join(repoRoot, "docs"), 0o755); err != nil {
		t.Fatalf("mkdir docs: %v", err)
	}

	files := map[string]string{
		filepath.Join(repoRoot, repositoryLayoutDocPath):           "layout",
		filepath.Join(moduleDir, goModuleFileName):                 "module github.com/shatter-dev/shatter/shatter-go\n",
		filepath.Join(moduleDir, mainFileName):                     "package main\n",
		filepath.Join(repoRoot, repositoryCacheDirectory, ".keep"): "",
	}
	for path, content := range files {
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatalf("mkdir for %q: %v", path, err)
		}
		if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
			t.Fatalf("write %q: %v", path, err)
		}
	}

	return repoRoot
}
