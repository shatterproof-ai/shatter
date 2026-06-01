package launcher

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"
)

func TestInvokeTimeoutRunsCleanupBeforeWaitingForStdout(t *testing.T) {
	dir := t.TempDir()
	childPIDFile := filepath.Join(dir, "child.pid")
	binaryPath := filepath.Join(dir, "fake-launcher")
	script := "#!/bin/sh\n" +
		"sleep 60 &\n" +
		"echo $! > " + strconv.Quote(childPIDFile) + "\n" +
		"read line\n" +
		"wait\n"
	if err := os.WriteFile(binaryPath, []byte(script), 0o755); err != nil {
		t.Fatalf("write fake launcher: %v", err)
	}

	sess, err := OpenSession(binaryPath)
	if err != nil {
		t.Fatalf("OpenSession: %v", err)
	}

	var cleanupOnce sync.Once
	cleanupCalled := make(chan struct{})
	cleanupChild := func() {
		cleanupOnce.Do(func() {
			close(cleanupCalled)
			var pidBytes []byte
			var err error
			for range 20 {
				pidBytes, err = os.ReadFile(childPIDFile)
				if err == nil {
					break
				}
				time.Sleep(10 * time.Millisecond)
			}
			if err != nil {
				return
			}
			pid, err := strconv.Atoi(strings.TrimSpace(string(pidBytes)))
			if err != nil {
				return
			}
			if proc, err := os.FindProcess(pid); err == nil {
				_ = proc.Kill()
			}
		})
	}
	sess.cleanup = func() error {
		cleanupChild()
		return nil
	}
	t.Cleanup(func() {
		cleanupChild()
		_ = sess.Close()
	})

	done := make(chan error, 1)
	go func() {
		_, err := sess.InvokeWithTimeout(LauncherRequest{
			Plan:    json.RawMessage(`{}`),
			Inputs:  []json.RawMessage{},
			Capture: false,
		}, 50*time.Millisecond)
		done <- err
	}()

	select {
	case err := <-done:
		if err == nil || !strings.Contains(err.Error(), "timed out") {
			t.Fatalf("InvokeWithTimeout error = %v, want timeout", err)
		}
		select {
		case <-cleanupCalled:
		default:
			t.Fatal("cleanup was not called on timeout")
		}
	case <-time.After(500 * time.Millisecond):
		cleanupChild()
		t.Fatal("InvokeWithTimeout blocked waiting for stdout before cleanup")
	}
}
