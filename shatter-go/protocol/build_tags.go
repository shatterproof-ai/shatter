package protocol

import (
	"fmt"
	"go/build"
	"go/build/constraint"
	"os"
	"path/filepath"
	"strings"
)

// goflagsTagsPrefixes lists the GOFLAGS forms that set the active build tag
// list. The Go toolchain accepts both "-tags=..." and "-tags ..." forms; both
// must be honored so files gated on those tags are visible to the analyzer
// when the user's standard toolchain env opts them in.
var goflagsTagsPrefixes = []string{"-tags=", "--tags="}

// goflagsTagsBareFlags lists the bare-flag forms that take their value from
// the next whitespace-separated token (e.g. "-tags foo,bar").
var goflagsTagsBareFlags = []string{"-tags", "--tags"}

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
// analyzer's active build context. When the file is excluded, the second
// return value is the raw constraint expression read from the file's
// header, or "" when the exclusion was driven by a filename suffix or the
// directive could not be located.
//
// Exclusion is detected via build.Context.MatchFile against a context whose
// BuildTags reflect the active GOFLAGS setting (str-jl9r). This keeps the
// upfront exclusion guard consistent with the underlying go/packages → go
// list path, which already honors GOFLAGS=-tags=... when Env is forwarded
// from os.Environ() in the loader. Without this synchronization a file
// gated on a tag the user opted into via GOFLAGS would be soft-skipped by
// the guard before go/packages got a chance to include it.
//
// MatchFile errors are treated as "not excluded" so genuine parse problems
// continue down the existing ParseError path instead of being hidden behind
// a build-tag soft-skip.
func isBuildTagExcluded(absoluteFilePath string) (bool, string) {
	dir, name := filepath.Split(absoluteFilePath)
	ctx := buildContextFromEnv(os.Getenv("GOFLAGS"))
	matches, err := ctx.MatchFile(dir, name)
	if err != nil {
		return false, ""
	}
	if matches {
		return false, ""
	}
	return true, readBuildConstraintExpression(absoluteFilePath)
}

// buildContextFromEnv returns a go/build.Context derived from build.Default
// with BuildTags extended by any tags declared in goflags (the value of the
// GOFLAGS environment variable). Unrecognized flags are ignored; only the
// "-tags=" / "--tags=" / "-tags <value>" forms are inspected, matching the
// shapes the Go toolchain itself accepts. Duplicate tags are deduplicated
// while preserving the original ordering.
func buildContextFromEnv(goflags string) build.Context {
	ctx := build.Default
	additional := parseGoflagsTags(goflags)
	if len(additional) == 0 {
		return ctx
	}
	merged := make([]string, 0, len(ctx.BuildTags)+len(additional))
	seen := make(map[string]struct{}, len(ctx.BuildTags)+len(additional))
	for _, tag := range ctx.BuildTags {
		if _, dup := seen[tag]; dup {
			continue
		}
		seen[tag] = struct{}{}
		merged = append(merged, tag)
	}
	for _, tag := range additional {
		if _, dup := seen[tag]; dup {
			continue
		}
		seen[tag] = struct{}{}
		merged = append(merged, tag)
	}
	ctx.BuildTags = merged
	return ctx
}

// parseGoflagsTags extracts the comma-separated tag list set via -tags or
// --tags inside the given GOFLAGS string. GOFLAGS uses whitespace-separated
// tokens; each token is either "<flag>=<value>" or a bare "<flag>" followed
// by a separate value token. Empty tag entries are dropped.
func parseGoflagsTags(goflags string) []string {
	if goflags == "" {
		return nil
	}
	tokens := strings.Fields(goflags)
	var rawValues []string
	for tokenIndex := 0; tokenIndex < len(tokens); tokenIndex++ {
		token := tokens[tokenIndex]
		matched := false
		for _, prefix := range goflagsTagsPrefixes {
			if strings.HasPrefix(token, prefix) {
				rawValues = append(rawValues, strings.TrimPrefix(token, prefix))
				matched = true
				break
			}
		}
		if matched {
			continue
		}
		for _, bare := range goflagsTagsBareFlags {
			if token == bare && tokenIndex+1 < len(tokens) {
				rawValues = append(rawValues, tokens[tokenIndex+1])
				tokenIndex++
				break
			}
		}
	}
	var tags []string
	for _, rawValue := range rawValues {
		for _, candidate := range strings.Split(rawValue, ",") {
			tag := strings.TrimSpace(candidate)
			if tag == "" {
				continue
			}
			tags = append(tags, tag)
		}
	}
	return tags
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
