package build

import (
	"os"
	"path/filepath"
	"strconv"
	"testing"
	"time"
)

func TestAcquireBuildGenerationLockRemovesFreshDeadPIDLock(t *testing.T) {
	root := t.TempDir()
	hash := "dead-pid-generation"
	lockPath := filepath.Join(root, hash+".build.lock")
	deadPID := strconv.Itoa(os.Getpid() + 1_000_000)
	if err := os.WriteFile(lockPath, []byte(deadPID+"\n"), 0o644); err != nil {
		t.Fatalf("write lock: %v", err)
	}

	done := make(chan error, 1)
	go func() {
		release, err := acquireBuildGenerationLock(root, hash)
		if release != nil {
			release()
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
