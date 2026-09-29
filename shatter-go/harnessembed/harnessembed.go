// Package harnessembed carries the shatter-harness runtime module inside the
// frontend binary and materializes it on disk for generated launcher builds.
//
// shatter-go/harness is its own Go module, so //go:embed cannot reach it from
// the shatter-go module. The two files are mirrored under mirror/ (as .txt so
// the toolchain neither treats them as a nested module nor compiles them) and
// kept in sync by `go generate`; TestMirrorMatchesHarnessModule fails on drift.
package harnessembed

//go:generate go run gen.go

import (
	"crypto/sha256"
	"embed"
	"encoding/hex"
	"errors"
	"fmt"
	"os"
	"path/filepath"
)

//go:embed mirror/go.mod.txt mirror/runtime.go.txt
var mirrorFS embed.FS

// files maps each materialized file name to its embedded mirror path.
var files = []struct{ name, embedded string }{
	{"go.mod", "mirror/go.mod.txt"},
	{"runtime.go", "mirror/runtime.go.txt"},
}

// Contents returns the embedded module files keyed by their materialized names.
func Contents() (map[string][]byte, error) {
	out := make(map[string][]byte, len(files))
	for _, f := range files {
		data, err := mirrorFS.ReadFile(f.embedded)
		if err != nil {
			return nil, fmt.Errorf("read embedded %s: %w", f.embedded, err)
		}
		out[f.name] = data
	}
	return out, nil
}

// Hash returns a short content hash identifying the embedded module version.
func Hash() (string, error) {
	contents, err := Contents()
	if err != nil {
		return "", err
	}
	h := sha256.New()
	for _, f := range files {
		fmt.Fprintf(h, "%s\x00%d\x00", f.name, len(contents[f.name]))
		h.Write(contents[f.name])
	}
	return hex.EncodeToString(h.Sum(nil))[:16], nil
}

// Materialize writes the embedded module to <root>/harness-runtime/<hash>/ and
// returns that directory. The write goes to a temp directory that is renamed
// into place, so concurrent callers and interrupted runs never expose a
// partial module. An already-materialized directory is reused.
func Materialize(root string) (string, error) {
	if root == "" {
		return "", errors.New("materialize harness runtime: empty root")
	}
	hash, err := Hash()
	if err != nil {
		return "", err
	}
	contents, err := Contents()
	if err != nil {
		return "", err
	}
	absRoot, err := filepath.Abs(root)
	if err != nil {
		return "", fmt.Errorf("materialize harness runtime: resolve root: %w", err)
	}

	parent := filepath.Join(absRoot, "harness-runtime")
	final := filepath.Join(parent, hash)
	if complete(final, contents) {
		return final, nil
	}
	if err := os.MkdirAll(parent, 0o755); err != nil {
		return "", fmt.Errorf("materialize harness runtime: %w", err)
	}
	tmp, err := os.MkdirTemp(parent, hash+".tmp-")
	if err != nil {
		return "", fmt.Errorf("materialize harness runtime: %w", err)
	}
	defer os.RemoveAll(tmp)

	for _, f := range files {
		if err := os.WriteFile(filepath.Join(tmp, f.name), contents[f.name], 0o644); err != nil {
			return "", fmt.Errorf("materialize harness runtime: write %s: %w", f.name, err)
		}
	}
	if err := os.Rename(tmp, final); err != nil {
		// Another process may have won the race; accept its copy if complete.
		if complete(final, contents) {
			return final, nil
		}
		// A damaged directory (never produced by this function's own atomic
		// publish, but possible after external tampering) is replaced.
		if rmErr := os.RemoveAll(final); rmErr != nil {
			return "", fmt.Errorf("materialize harness runtime: clear damaged %s: %w", final, rmErr)
		}
		if err := os.Rename(tmp, final); err != nil {
			if complete(final, contents) {
				return final, nil
			}
			return "", fmt.Errorf("materialize harness runtime: publish %s: %w", final, err)
		}
	}
	return final, nil
}

// complete reports whether dir already holds every embedded file verbatim.
func complete(dir string, contents map[string][]byte) bool {
	for _, f := range files {
		got, err := os.ReadFile(filepath.Join(dir, f.name))
		if err != nil || string(got) != string(contents[f.name]) {
			return false
		}
	}
	return true
}
