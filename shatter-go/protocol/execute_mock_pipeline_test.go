package protocol

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

// TestExecute_ConfigMockSubstitution_PipelineSeam is the str-c8djq review
// fix 6 pipeline-seam test: it drives the real handler (analyze-free direct
// execute) end-to-end against a temp module carrying a real
// `.shatter/config.yaml`, and asserts the substituted mock value in the
// execute RESPONSE — not just a unit of the mechanism. It also confirms the
// real constructor's filesystem side effect never runs, proving the config
// mock takes effect through the full frontend pipeline (config load →
// type-resolved substitution → overlay build → launcher execution).
func TestExecute_ConfigMockSubstitution_PipelineSeam(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go toolchain unavailable")
	}

	modDir := t.TempDir()
	sentinel := filepath.Join(t.TempDir(), "real-ran.txt")

	writeFile(t, filepath.Join(modDir, "go.mod"), "module example.com/imp\n\ngo 1.23\n")

	depDir := filepath.Join(modDir, "dep")
	if err := os.MkdirAll(depDir, 0o755); err != nil {
		t.Fatal(err)
	}
	writeFile(t, filepath.Join(depDir, "dep.go"), `package dep

import "os"

type Thing struct{ N int }

func NewThing(sentinel string) *Thing {
	_ = os.WriteFile(sentinel, []byte("real"), 0o644)
	return &Thing{N: 5}
}
`)

	target := filepath.Join(modDir, "target.go")
	writeFile(t, target, `package main

import "example.com/imp/dep"

const sentinelPath = `+"`"+sentinel+"`"+`

func Classify(n int) int {
	t := dep.NewThing(sentinelPath)
	if t == nil {
		return -1
	}
	if n > 0 {
		return t.N
	}
	return 0
}

func main() {}
`)

	shatterDir := filepath.Join(modDir, ".shatter")
	if err := os.MkdirAll(shatterDir, 0o755); err != nil {
		t.Fatal(err)
	}
	writeFile(t, filepath.Join(shatterDir, "config.yaml"), `functions:
  "target.go:Classify":
    mocks:
      "dep.NewThing": "&dep.Thing{N: 99}"
`)

	req := reqJSON(1, "execute", fmt.Sprintf(`"file":"%s","function":"Classify","inputs":[7]`, target))
	resp := sendRecv(t, req)

	if resp.Status != "execute" {
		t.Fatalf("status = %q, want execute (message: %s)", resp.Status, resp.Message)
	}
	if resp.Outcome == nil || resp.Outcome.Status != OutcomeStatusCompleted {
		t.Fatalf("expected completed outcome, got %+v (message: %s)", resp.Outcome, resp.Message)
	}
	var got int
	if err := json.Unmarshal([]byte(strings.TrimSpace(string(resp.Outcome.ReturnValue))), &got); err != nil {
		t.Fatalf("unmarshal return value %q: %v", resp.Outcome.ReturnValue, err)
	}
	if got != 99 {
		t.Fatalf("Classify(7) = %d, want 99 (config mock value); real constructor returns 5", got)
	}
	if _, err := os.Stat(sentinel); !os.IsNotExist(err) {
		t.Fatalf("sentinel exists (%v): real constructor side effect was not suppressed by the config mock", err)
	}
}

func writeFile(t *testing.T, path, content string) {
	t.Helper()
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatalf("write %s: %v", path, err)
	}
}
