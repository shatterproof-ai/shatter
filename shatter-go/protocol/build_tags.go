package protocol

import (
	"fmt"
	"go/build"
	"go/build/constraint"
	"path/filepath"
	"strings"
)

// buildConstraintScanLimit caps how many leading bytes of a file are read
// when looking for a //go:build (or legacy // +build) directive. Build
// constraints are required to appear before the package clause in real Go
// source, so a few kilobytes is more than enough to surface them while
// avoiding the cost of reading large generated files end-to-end.
const buildConstraintScanLimit = 8 * 1024

// BuildTagExcludedError is returned by the analyzer when the target file is
// excluded from the active Go build context (for example a file gated by
// //go:build ui when the analyzer's tags do not include "ui"). The handler
// maps this to ErrNotSupported so the Rust core's batch_analyze path
// soft-skips the file rather than aborting the run on a generic
// ParseError. See str-8amu.
type BuildTagExcludedError struct {
	// Path is the absolute path of the excluded file.
	Path string
	// Constraint is the parsed build-constraint expression read from the
	// file (e.g., "ui" or "linux && amd64"). Empty when the exclusion comes
	// from a filename suffix such as "_windows.go" or when no constraint
	// directive could be located in the scanned region.
	Constraint string
}

func (e *BuildTagExcludedError) Error() string {
	if e == nil {
		return "build-tag-excluded"
	}
	if e.Constraint != "" {
		return fmt.Sprintf("build-tag-excluded: %s (constraint: %s)", e.Path, e.Constraint)
	}
	return fmt.Sprintf("build-tag-excluded: %s", e.Path)
}

// isBuildTagExcluded reports whether absoluteFilePath is excluded from the
// analyzer's default build context. When the file is excluded, the second
// return value is the raw constraint expression read from the file's
// header, or "" when the exclusion was driven by a filename suffix or the
// directive could not be located.
//
// Exclusion is detected via go/build.Default.MatchFile, which honors both
// //go:build directives and OS/arch filename suffixes (e.g., *_windows.go).
// MatchFile errors are treated as "not excluded" so that genuine parse
// problems continue down the existing ParseError path instead of being
// hidden behind a build-tag soft-skip.
func isBuildTagExcluded(absoluteFilePath string) (bool, string) {
	dir, name := filepath.Split(absoluteFilePath)
	matches, err := build.Default.MatchFile(dir, name)
	if err != nil {
		return false, ""
	}
	if matches {
		return false, ""
	}
	return true, readBuildConstraintExpression(absoluteFilePath)
}

// readBuildConstraintExpression scans the head of absoluteFilePath for a
// //go:build (preferred) or legacy // +build directive and returns the
// parsed constraint expression as a string. Returns "" when no directive
// is found within the scanned region or the file cannot be read.
func readBuildConstraintExpression(absoluteFilePath string) string {
	header, err := readFileHeader(absoluteFilePath, buildConstraintScanLimit)
	if err != nil {
		return ""
	}
	for _, rawLine := range strings.Split(string(header), "\n") {
		line := strings.TrimRight(rawLine, "\r")
		trimmed := strings.TrimSpace(line)
		if trimmed == "" {
			continue
		}
		if !strings.HasPrefix(trimmed, "//") {
			// Build-constraint directives must precede the package clause
			// in valid Go source. Stop once a non-comment line is reached.
			return ""
		}
		if constraint.IsGoBuild(trimmed) || constraint.IsPlusBuild(trimmed) {
			expr, err := constraint.Parse(trimmed)
			if err != nil {
				return ""
			}
			return expr.String()
		}
	}
	return ""
}
