// Package overlay generates `go build -overlay` manifests.
//
// The manifest is a JSON document {"Replace": {<in-tree>: <real>}} consumed
// by `go build -overlay <file>`. Shatter uses it to splice generated wrappers,
// launchers, and instrumented sources into a target module without mutating
// its source tree.
package overlay

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
)

// Manifest is the JSON shape consumed by `go build -overlay <file>`.
type Manifest struct {
	Replace map[string]string `json:"Replace"`
}

// Builder accumulates in-tree → real-file mappings and writes the manifest.
type Builder struct {
	overlaysDir string
	planID      string
	replace     map[string]string
}

// NewBuilder returns a Builder that will write the manifest to
// <overlaysDir>/<planID>.json when Write is called.
func NewBuilder(overlaysDir, planID string) *Builder {
	return &Builder{
		overlaysDir: overlaysDir,
		planID:      planID,
		replace:     make(map[string]string),
	}
}

// Add maps an in-tree source path to a real file on disk. Both paths are
// converted to absolute form. Adding the same (inTreePath, realPath) pair
// twice is a no-op; adding a different realPath for the same inTreePath
// returns an error.
//
// Real-file existence is not checked here — it is deferred to Write so
// callers may register paths that are still being generated.
func (b *Builder) Add(inTreePath, realPath string) error {
	if inTreePath == "" {
		return fmt.Errorf("overlay: Add: inTreePath must not be empty")
	}
	if realPath == "" {
		return fmt.Errorf("overlay: Add: realPath must not be empty")
	}

	absIn, err := filepath.Abs(inTreePath)
	if err != nil {
		return fmt.Errorf("overlay: Add: absolutize inTreePath %q: %w", inTreePath, err)
	}
	absReal, err := filepath.Abs(realPath)
	if err != nil {
		return fmt.Errorf("overlay: Add: absolutize realPath %q: %w", realPath, err)
	}

	if existing, ok := b.replace[absIn]; ok {
		if existing == absReal {
			return nil
		}
		return fmt.Errorf(
			"overlay: Add: in-tree path %q already mapped to %q, refusing to remap to %q",
			absIn, existing, absReal,
		)
	}
	b.replace[absIn] = absReal
	return nil
}

// AddGenerated registers a generated file. The synthetic in-tree path is
// computed as filepath.Join(anchor, basename); callers that need a
// subdirectory under the anchor may embed it in basename
// (e.g. "shatter_launcher_<hash>/main.go").
func (b *Builder) AddGenerated(realFile, anchor, basename string) error {
	if realFile == "" {
		return fmt.Errorf("overlay: AddGenerated: realFile must not be empty")
	}
	if anchor == "" {
		return fmt.Errorf("overlay: AddGenerated: anchor must not be empty")
	}
	if basename == "" {
		return fmt.Errorf("overlay: AddGenerated: basename must not be empty")
	}
	inTree := filepath.Join(anchor, basename)
	return b.Add(inTree, realFile)
}

// Write asserts every registered real file exists as a regular file, ensures
// the overlays directory exists, and writes the manifest JSON to
// <overlaysDir>/<planID>.json. The write is atomic (write-then-rename).
// Returns the absolute path of the manifest file.
func (b *Builder) Write() (string, error) {
	if b.overlaysDir == "" {
		return "", fmt.Errorf("overlay: Write: overlaysDir must not be empty")
	}
	if b.planID == "" {
		return "", fmt.Errorf("overlay: Write: planID must not be empty")
	}

	for inTree, realPath := range b.replace {
		info, err := os.Stat(realPath)
		if err != nil {
			return "", fmt.Errorf(
				"overlay: Write: real file for %q: stat %q: %w",
				inTree, realPath, err,
			)
		}
		if !info.Mode().IsRegular() {
			return "", fmt.Errorf(
				"overlay: Write: real file for %q: %q is not a regular file",
				inTree, realPath,
			)
		}
	}

	if err := os.MkdirAll(b.overlaysDir, 0o755); err != nil {
		return "", fmt.Errorf("overlay: Write: create overlays dir %q: %w", b.overlaysDir, err)
	}

	absDir, err := filepath.Abs(b.overlaysDir)
	if err != nil {
		return "", fmt.Errorf("overlay: Write: absolutize overlaysDir %q: %w", b.overlaysDir, err)
	}
	finalPath := filepath.Join(absDir, b.planID+".json")
	tmpPath := finalPath + ".tmp"

	manifest := Manifest{Replace: b.replace}
	payload, err := json.MarshalIndent(manifest, "", "  ")
	if err != nil {
		return "", fmt.Errorf("overlay: Write: marshal manifest: %w", err)
	}
	payload = append(payload, '\n')

	if err := os.WriteFile(tmpPath, payload, 0o644); err != nil {
		return "", fmt.Errorf("overlay: Write: write temp manifest %q: %w", tmpPath, err)
	}
	if err := os.Rename(tmpPath, finalPath); err != nil {
		_ = os.Remove(tmpPath)
		return "", fmt.Errorf("overlay: Write: rename %q -> %q: %w", tmpPath, finalPath, err)
	}
	return finalPath, nil
}
