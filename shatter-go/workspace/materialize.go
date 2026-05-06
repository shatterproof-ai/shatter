package workspace

import (
	"errors"
	"fmt"
	"os"
)

// ErrEmptyMaterializedFile is returned when the workspace materialization
// pipeline writes a zero-byte source file even though the producer
// expected non-empty contents. Catching this before `go build` runs
// converts an opaque "expected package, found EOF" Go compiler diagnostic
// into a specific workspace/materialization preflight failure (str-jeen.51).
var ErrEmptyMaterializedFile = errors.New("workspace: materialized source file is empty")

// VerifyMaterializedSource asserts that path exists, is a regular file,
// and (when expectNonEmpty is true) has non-zero size. It is intended for
// post-write verification of files copied or generated as part of the Go
// frontend's workspace materialization pipeline (instrumented overlay
// sources, package rewrites, generated runtime helpers).
//
// On a zero-byte file with expectNonEmpty=true the returned error wraps
// ErrEmptyMaterializedFile so callers can classify the failure as a
// preflight materialization problem rather than a Go compiler error.
func VerifyMaterializedSource(path string, expectNonEmpty bool) error {
	info, err := os.Stat(path)
	if err != nil {
		return fmt.Errorf("workspace: stat materialized source %q: %w", path, err)
	}
	if !info.Mode().IsRegular() {
		return fmt.Errorf("workspace: materialized source %q is not a regular file", path)
	}
	if expectNonEmpty && info.Size() == 0 {
		return fmt.Errorf("%w: %s", ErrEmptyMaterializedFile, path)
	}
	return nil
}
