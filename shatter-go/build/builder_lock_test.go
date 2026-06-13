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

func TestBuildGenerationLockOwnedByLivePIDIsNotStaleAfterAgeThreshold(t *testing.T) {
	root := t.TempDir()
	hash := "live-pid-generation"
	lockPath := filepath.Join(root, hash+".build.lock")
	if err := os.WriteFile(lockPath, []byte(strconv.Itoa(os.Getpid())+"\n"), 0o644); err != nil {
		t.Fatalf("write lock: %v", err)
	}
	old := time.Now().Add(-(buildGenerationLockStaleAfter + time.Minute))
	if err := os.Chtimes(lockPath, old, old); err != nil {
		t.Fatalf("age lock: %v", err)
	}

	if buildGenerationLockIsStale(lockPath) {
		t.Fatal("lock owned by a live process must not be treated as stale")
	}
}
