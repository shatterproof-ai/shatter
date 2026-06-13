package launcher

import (
	"os"
	"path/filepath"
	"strconv"
	"testing"
	"time"
)

func TestAcquireLauncherBuildLockRemovesFreshDeadPIDLock(t *testing.T) {
	binaryPath := filepath.Join(t.TempDir(), "shatter_launcher_deadpid")
	lockPath := binaryPath + ".lock"
	deadPID := strconv.Itoa(os.Getpid() + 1_000_000)
	if err := os.WriteFile(lockPath, []byte(deadPID+"\n"), 0o644); err != nil {
		t.Fatalf("write lock: %v", err)
	}

	done := make(chan error, 1)
	go func() {
		release, acquired, err := acquireLauncherBuildLock(binaryPath)
		if release != nil {
			release()
		}
		if err == nil && !acquired {
			err = os.ErrExist
		}
		done <- err
	}()

	select {
	case err := <-done:
		if err != nil {
			t.Fatalf("acquire lock: %v", err)
		}
	case <-time.After(2 * time.Second):
		_ = os.Remove(lockPath)
		if err := <-done; err != nil {
			t.Fatalf("acquire after cleanup: %v", err)
		}
		t.Fatal("fresh lock from dead PID blocked instead of being removed")
	}
}
