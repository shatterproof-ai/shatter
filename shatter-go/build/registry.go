package build

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sync"
)

const registryFileName = "binary_registry.json"

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
		return r.save()
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

func (r *BinaryRegistry) save() error {
	dir := filepath.Dir(r.persistPath)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		return fmt.Errorf("registry: mkdir %q: %w", dir, err)
	}
	data, err := json.MarshalIndent(r.index, "", "  ")
	if err != nil {
		return fmt.Errorf("registry: marshal: %w", err)
	}
	tmp := r.persistPath + ".tmp"
	if err := os.WriteFile(tmp, data, 0o644); err != nil {
		return fmt.Errorf("registry: write tmp: %w", err)
	}
	if err := os.Rename(tmp, r.persistPath); err != nil {
		_ = os.Remove(tmp)
		return fmt.Errorf("registry: rename: %w", err)
	}
	return nil
}
