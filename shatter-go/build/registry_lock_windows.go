//go:build windows

package build

import (
	"fmt"
	"os"
	"path/filepath"

	"golang.org/x/sys/windows"
)

func acquireRegistryPersistenceLock(stateDir string) (func(), error) {
	lockPath := filepath.Join(stateDir, registryFileName+".lock")
	lockFile, err := os.OpenFile(lockPath, os.O_CREATE|os.O_RDWR, 0o644)
	if err != nil {
		return nil, fmt.Errorf("open %q: %w", lockPath, err)
	}

	overlapped := new(windows.Overlapped)
	if err := windows.LockFileEx(
		windows.Handle(lockFile.Fd()),
		windows.LOCKFILE_EXCLUSIVE_LOCK,
		0,
		1,
		0,
		overlapped,
	); err != nil {
		_ = lockFile.Close()
		return nil, fmt.Errorf("lock %q: %w", lockPath, err)
	}
	return func() {
		_ = windows.UnlockFileEx(windows.Handle(lockFile.Fd()), 0, 1, 0, overlapped)
		_ = lockFile.Close()
	}, nil
}
