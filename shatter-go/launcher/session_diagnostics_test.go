package launcher_test

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/launcher"
)

// TestInvokeSurfacesStderrOnSubprocessExit is the regression for
// str-jeen.80: when the launcher subprocess exits before producing a
// response, the returned error must carry the binary path, exit status,
// and captured stderr so the CLI emits a structured diagnostic instead
// of an opaque "subprocess exited unexpectedly".
func TestInvokeSurfacesStderrOnSubprocessExit(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("uses POSIX shell")
	}
	dir := t.TempDir()
	binaryPath := filepath.Join(dir, "fake-launcher")
	script := "#!/bin/sh\nread line\necho 'fatal: bad config 12345' >&2\nexit 7\n"
	if err := os.WriteFile(binaryPath, []byte(script), 0o755); err != nil {
		t.Fatalf("write fake launcher: %v", err)
	}
	sess, err := launcher.OpenSession(binaryPath)
	if err != nil {
		t.Fatalf("OpenSession: %v", err)
	}
	t.Cleanup(func() { _ = sess.Close() })

	_, err = sess.Invoke(launcher.LauncherRequest{})
	if err == nil {
		t.Fatal("expected error from Invoke when subprocess exits")
	}
	msg := err.Error()
	for _, want := range []string{
		"subprocess exited",
		"fatal: bad config 12345",
		binaryPath,
		"exit status 7",
	} {
		if !strings.Contains(msg, want) {
			t.Errorf("error message missing %q\nfull error: %s", want, msg)
		}
	}
}
