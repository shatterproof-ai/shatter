package main

import (
	"bufio"
	"encoding/json"
	"fmt"
	"io/fs"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/protocol"
)

// copyTree copies src to dst, skipping VCS and build-output directories.
func copyTree(t *testing.T, src, dst string) {
	t.Helper()
	err := filepath.WalkDir(src, func(path string, d fs.DirEntry, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		rel, err := filepath.Rel(src, path)
		if err != nil {
			return err
		}
		if d.IsDir() {
			switch d.Name() {
			case ".git", "bin", "node_modules", "testdata":
				return filepath.SkipDir
			}
			return os.MkdirAll(filepath.Join(dst, rel), 0o755)
		}
		if !d.Type().IsRegular() {
			return nil
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		return os.WriteFile(filepath.Join(dst, rel), data, 0o644)
	})
	if err != nil {
		t.Fatalf("copy %s -> %s: %v", src, dst, err)
	}
}

// TestFrontendExecutesAfterSourceTreeDeleted is the str-49drv.100 relocation
// regression: a frontend built with -trimpath from a temporary copy of
// shatter-go, whose copy is then deleted, must still execute a target from a
// cold workspace. The harness runtime therefore cannot come from the
// compile-time source path.
func TestFrontendExecutesAfterSourceTreeDeleted(t *testing.T) {
	if testing.Short() {
		t.Skip("builds the frontend and a cold launcher; skipped in -short")
	}

	_, thisFile, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("runtime.Caller(0) failed")
	}
	srcRoot := filepath.Dir(thisFile)

	scratch := t.TempDir()
	buildCopy := filepath.Join(scratch, "build-copy")
	binPath := filepath.Join(scratch, "shatter-go-reloc")
	copyTree(t, srcRoot, buildCopy)

	build := exec.Command("go", "build", "-trimpath", "-buildvcs=false", "-o", binPath, ".")
	build.Dir = buildCopy
	if out, err := build.CombinedOutput(); err != nil {
		t.Fatalf("go build of relocated copy: %v\n%s", err, out)
	}
	// Delete (not move) the copy so the compiled-in source path exists nowhere.
	if err := os.RemoveAll(buildCopy); err != nil {
		t.Fatalf("remove build copy: %v", err)
	}

	target := filepath.Join(scratch, "target")
	if err := os.MkdirAll(target, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(target, "go.mod"), []byte("module reloctarget\n\ngo 1.23\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	targetFile := filepath.Join(target, "double.go")
	if err := os.WriteFile(targetFile, []byte("package reloctarget\n\nfunc Double(x int) int { return x * 2 }\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	requests := strings.Join([]string{
		fmt.Sprintf(`{"protocol_version":%q,"id":1,"command":"handshake","capabilities":["analyze"]}`, protocol.ProtocolVersion),
		fmt.Sprintf(`{"protocol_version":%q,"id":2,"command":"analyze","file":%q}`, protocol.ProtocolVersion, targetFile),
		fmt.Sprintf(`{"protocol_version":%q,"id":3,"command":"execute","file":%q,"function":"Double","inputs":[5]}`, protocol.ProtocolVersion, targetFile),
		fmt.Sprintf(`{"protocol_version":%q,"id":4,"command":"shutdown"}`, protocol.ProtocolVersion),
	}, "\n") + "\n"

	cmd := exec.Command(binPath)
	cmd.Dir = t.TempDir() // outside the repo and the target module
	cmd.Env = append(os.Environ(), "SHATTER_GO_WORKSPACE_ROOT="+t.TempDir())
	cmd.Stdin = strings.NewReader(requests)
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		t.Fatal(err)
	}
	if err := cmd.Start(); err != nil {
		t.Fatalf("start relocated frontend: %v", err)
	}

	var executeLine string
	scanner := bufio.NewScanner(stdout)
	scanner.Buffer(make([]byte, 0, 1<<20), 16<<20)
	for scanner.Scan() {
		var head struct {
			ID int `json:"id"`
		}
		if json.Unmarshal(scanner.Bytes(), &head) == nil && head.ID == 3 {
			executeLine = scanner.Text()
		}
	}
	if err := scanner.Err(); err != nil {
		t.Fatalf("read frontend stdout: %v", err)
	}
	if err := cmd.Wait(); err != nil {
		t.Fatalf("relocated frontend exited: %v", err)
	}
	if executeLine == "" {
		t.Fatal("no execute response received")
	}

	var resp struct {
		Status      string          `json:"status"`
		ReturnValue json.RawMessage `json:"return_value"`
		Message     string          `json:"message"`
		Outcome     json.RawMessage `json:"outcome"`
	}
	if err := json.Unmarshal([]byte(executeLine), &resp); err != nil {
		t.Fatalf("decode execute response %q: %v", executeLine, err)
	}
	if resp.Status != "execute" || strings.TrimSpace(string(resp.ReturnValue)) != "10" {
		t.Fatalf("execute(Double, [5]) = status %q return %s; want status execute return 10\nresponse: %s",
			resp.Status, resp.ReturnValue, executeLine)
	}
}
