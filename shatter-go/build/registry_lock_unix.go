//go:build !windows

package build

import (
	"fmt"
	"os"
	"path/filepath"
	"syscall"
)

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
