package build

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"syscall"
)

const registryFileName = "binary_registry.json"

const registryStateDirectorySuffix = "-registry-state"

// BinaryRegistry is a thread-safe in-memory registry of compiled launcher
// binaries, keyed by discovery hash. It also persists the index to disk so
// that a freshly constructed Registry can recover cached binaries from a
// prior process run.
type BinaryRegistry struct {
	mu          sync.Mutex
	index       map[string]string // discoveryHash → binaryPath
	persistPath string
}

// NewBinaryRegistry creates an in-memory registry. If persistDir is non-empty
// the registry loads any previously persisted entries from
// <persistDir>/binary_registry.json and appends new entries there on Register.
func NewBinaryRegistry(persistDir string) *BinaryRegistry {
	r := &BinaryRegistry{
		index: make(map[string]string),
	}
	if persistDir != "" {
		r.persistPath = filepath.Join(persistDir, registryFileName)
		_ = r.load()
	}
	return r
}

// Lookup returns the binary path for the given discovery hash and whether it
// was found. A found entry is only valid if the binary still exists on disk;
// stale entries (binary deleted) are evicted and false is returned.
func (r *BinaryRegistry) Lookup(discoveryHash string) (binaryPath string, ok bool) {
	r.mu.Lock()
	defer r.mu.Unlock()
	path, found := r.index[discoveryHash]
	if !found {
		return "", false
	}
	if _, err := os.Stat(path); err != nil {
		delete(r.index, discoveryHash)
		return "", false
	}
	return path, true
}

// Register stores the binary path for a discovery hash and persists the index.
func (r *BinaryRegistry) Register(discoveryHash, binaryPath string) error {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.index[discoveryHash] = binaryPath
	if r.persistPath != "" {
		return r.save(discoveryHash, binaryPath)
	}
	return nil
}

// Len returns the number of entries in the registry.
func (r *BinaryRegistry) Len() int {
	r.mu.Lock()
	defer r.mu.Unlock()
	return len(r.index)
}

func (r *BinaryRegistry) load() error {
	data, err := os.ReadFile(r.persistPath)
	if err != nil {
		return nil // file not yet present; not an error
	}
	return json.Unmarshal(data, &r.index)
}

func (r *BinaryRegistry) save(discoveryHash, binaryPath string) error {
	dir := filepath.Dir(r.persistPath)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return fmt.Errorf("registry: mkdir %q: %w", dir, err)
	}
	stateDir := registryStateDir(dir)
	if err := os.MkdirAll(stateDir, 0o755); err != nil {
		return fmt.Errorf("registry: mkdir state dir %q: %w", stateDir, err)
	}
	release, err := acquireRegistryPersistenceLock(stateDir)
	if err != nil {
		return fmt.Errorf("registry: acquire persistence lock: %w", err)
	}
	defer release()

	persisted := make(map[string]string)
	data, err := os.ReadFile(r.persistPath)
	if err == nil {
		if err := json.Unmarshal(data, &persisted); err != nil {
			return fmt.Errorf("registry: decode persisted index: %w", err)
		}
	} else if !os.IsNotExist(err) {
		return fmt.Errorf("registry: read persisted index: %w", err)
	}
	// Persist only this Register mutation. The receiver may have loaded its
	// in-memory snapshot before another process updated the file; merging the
	// whole snapshot here would roll those newer values back.
	persisted[discoveryHash] = binaryPath

	data, err = json.MarshalIndent(persisted, "", "  ")
	if err != nil {
		return fmt.Errorf("registry: marshal: %w", err)
	}
	// RunGC manages binaries/, so both synchronization and scratch files live
	// in a sibling state directory. The sibling remains on the same filesystem,
	// preserving atomic rename into the final registry path.
	tmpFile, err := os.CreateTemp(stateDir, registryFileName+".*.tmp")
	if err != nil {
		return fmt.Errorf("registry: create tmp: %w", err)
	}
	tmp := tmpFile.Name()
	defer func() { _ = os.Remove(tmp) }()
	if err := tmpFile.Chmod(0o644); err != nil {
		_ = tmpFile.Close()
		return fmt.Errorf("registry: chmod tmp: %w", err)
	}
	if _, err := tmpFile.Write(data); err != nil {
		_ = tmpFile.Close()
		return fmt.Errorf("registry: write tmp: %w", err)
	}
	if err := tmpFile.Close(); err != nil {
		return fmt.Errorf("registry: close tmp: %w", err)
	}
	if err := os.Rename(tmp, r.persistPath); err != nil {
		return fmt.Errorf("registry: rename: %w", err)
	}
	r.index = persisted
	return nil
}

func registryStateDir(persistDir string) string {
	return filepath.Join(
		filepath.Dir(persistDir),
		"."+filepath.Base(persistDir)+registryStateDirectorySuffix,
	)
}

func acquireRegistryPersistenceLock(stateDir string) (func(), error) {
	lockPath := filepath.Join(stateDir, registryFileName+".lock")
	lockFile, err := os.OpenFile(lockPath, os.O_CREATE|os.O_RDWR, 0o644)
	if err != nil {
		return nil, fmt.Errorf("open %q: %w", lockPath, err)
	}
	if err := syscall.Flock(int(lockFile.Fd()), syscall.LOCK_EX); err != nil {
		_ = lockFile.Close()
		return nil, fmt.Errorf("lock %q: %w", lockPath, err)
	}
	return func() {
		_ = syscall.Flock(int(lockFile.Fd()), syscall.LOCK_UN)
		_ = lockFile.Close()
	}, nil
}
