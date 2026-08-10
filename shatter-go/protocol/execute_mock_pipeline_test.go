package protocol

import (
	"encoding/json"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
	"time"
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

// TestExecute_PrepareIDPicksUpConfigMockEdit_PipelineSeam is the str-hr40t
// acceptance test at the pipeline seam: a long-lived handler prepares a
// harness, the operator edits `.shatter/config.yaml` mocks, and a later
// execute that names the SAME prepare_id must reflect the new substitution.
// Before the fix the stale harness kept serving the old expression because the
// explicit prepare_id path never compared mock fingerprints.
func TestExecute_PrepareIDPicksUpConfigMockEdit_PipelineSeam(t *testing.T) {
	if _, err := exec.LookPath("go"); err != nil {
		t.Skip("go toolchain unavailable")
	}

	modDir := t.TempDir()
	writeFile(t, filepath.Join(modDir, "go.mod"), "module example.com/imp\n\ngo 1.23\n")

	depDir := filepath.Join(modDir, "dep")
	if err := os.MkdirAll(depDir, 0o755); err != nil {
		t.Fatal(err)
	}
	writeFile(t, filepath.Join(depDir, "dep.go"), `package dep

type Thing struct{ N int }

func NewThing() *Thing { return &Thing{N: 5} }
`)

	target := filepath.Join(modDir, "target.go")
	writeFile(t, target, `package main

import "example.com/imp/dep"

func Classify(n int) int {
	t := dep.NewThing()
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
	cfgPath := filepath.Join(shatterDir, "config.yaml")
	writeConfigMock := func(expr string) {
		t.Helper()
		writeFile(t, cfgPath, `functions:
  "target.go:Classify":
    mocks:
      "dep.NewThing": "`+expr+`"
`)
		// The parsed-config cache keys on mtime + size; the two revisions have
		// equal length, so bump mtime explicitly rather than relying on
		// filesystem timestamp granularity.
		future := time.Now().Add(3 * time.Second)
		if err := os.Chtimes(cfgPath, future, future); err != nil {
			t.Fatal(err)
		}
	}
	writeConfigMock("&dep.Thing{N: 99}")

	h := NewHandlerWithLogLevel(strings.NewReader(""), io.Discard, io.Discard, "error")
	fnName := "Classify"
	prepResp := h.handlePrepare(Response{ProtocolVersion: ProtocolVersion, ID: 1}, Request{
		ProtocolVersion: ProtocolVersion, ID: 1, Command: "prepare", File: target, Function: &fnName,
	})
	if prepResp.Status != "prepare" || prepResp.PrepareID == "" {
		t.Fatalf("prepare status = %q id = %q (message: %s)", prepResp.Status, prepResp.PrepareID, prepResp.Message)
	}
	prepareID := prepResp.PrepareID
	staleHarness := h.preparedHarnesses[prepareID]
	if staleHarness == nil {
		t.Fatalf("prepare did not cache a harness under %q", prepareID)
	}

	execWithPrepareID := func(id int) Response {
		t.Helper()
		return h.handleExecute(Response{ProtocolVersion: ProtocolVersion, ID: id}, Request{
			ProtocolVersion: ProtocolVersion, ID: id, Command: "execute", File: target,
			Function: &fnName, Inputs: []json.RawMessage{json.RawMessage(`7`)}, PrepareID: &prepareID,
		})
	}
	returned := func(resp Response) int {
		t.Helper()
		if resp.Status != "execute" || resp.Outcome == nil || resp.Outcome.Status != OutcomeStatusCompleted {
			t.Fatalf("execute status = %q outcome = %+v (message: %s)", resp.Status, resp.Outcome, resp.Message)
		}
		var got int
		if err := json.Unmarshal([]byte(strings.TrimSpace(string(resp.Outcome.ReturnValue))), &got); err != nil {
			t.Fatalf("unmarshal return value %q: %v", resp.Outcome.ReturnValue, err)
		}
		return got
	}

	if got := returned(execWithPrepareID(2)); got != 99 {
		t.Fatalf("first execute = %d, want 99 (config mock value)", got)
	}

	// Operator edits the config mid-session.
	writeConfigMock("&dep.Thing{N: 77}")

	if got := returned(execWithPrepareID(3)); got != 77 {
		t.Fatalf("execute after config edit = %d, want 77; the stale prepared harness was reused", got)
	}
	if _, still := h.preparedHarnesses[prepareID]; still {
		t.Errorf("stale prepare_id %q must be evicted after the config edit", prepareID)
	}
	for id, harness := range h.preparedHarnesses {
		if harness == staleHarness {
			t.Errorf("stale harness is still registered under %q", id)
		}
	}
	// Cleanup() closes the launcher session; observing it nil is how the test
	// sees that the stale harness was released rather than merely unregistered.
	// (IsValid() is not a proxy here: the launcher binary lives in the shared
	// build cache, so it survives artifact-dir removal.)
	if pl, ok := staleHarness.(*preparedLauncher); ok {
		pl.mu.Lock()
		session := pl.session
		pl.mu.Unlock()
		if session != nil {
			t.Errorf("stale harness launcher session must be closed by Cleanup")
		}
	}

	// The rebuild must be re-registered under the new fingerprint, not torn
	// down as a one-shot: otherwise every subsequent execute on this prepare_id
	// rebuilds from scratch for the rest of the session (str-hr40t review).
	if len(h.preparedHarnesses) != 1 {
		t.Fatalf("expected exactly one cached harness after the rebuild, got %d", len(h.preparedHarnesses))
	}
	var rebuilt preparedExecution
	for _, harness := range h.preparedHarnesses {
		rebuilt = harness
	}

	// A THIRD execute with the same (now-stale) prepare_id must reuse the
	// rebuilt harness rather than build again.
	if got := returned(execWithPrepareID(4)); got != 77 {
		t.Fatalf("second execute after config edit = %d, want 77", got)
	}
	if len(h.preparedHarnesses) != 1 {
		t.Fatalf("second post-edit execute changed the cache size to %d", len(h.preparedHarnesses))
	}
	for _, harness := range h.preparedHarnesses {
		if harness != rebuilt {
			t.Errorf("second post-edit execute rebuilt the harness instead of reusing the cached one")
		}
	}
	// A rebuilt-then-discarded harness would have been Cleanup()'d, closing its
	// session; a genuinely reused one still holds its launcher session open.
	if pl, ok := rebuilt.(*preparedLauncher); ok {
		pl.mu.Lock()
		session := pl.session
		pl.mu.Unlock()
		if session == nil {
			t.Errorf("rebuilt harness session was torn down; it was treated as one-shot, not re-cached")
		}
	}
}

func writeFile(t *testing.T, path, content string) {
	t.Helper()
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatalf("write %s: %v", path, err)
	}
}
