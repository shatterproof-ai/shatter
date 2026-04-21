package build

import (
	"fmt"
	"strconv"
	"strings"
)

// DiagnosticKind classifies a compiler diagnostic.
type DiagnosticKind string

const (
	DiagnosticKindError   DiagnosticKind = "error"
	DiagnosticKindWarning DiagnosticKind = "warning"
)

// Diagnostic is a structured compiler message extracted from go build output.
type Diagnostic struct {
	Kind    DiagnosticKind `json:"kind"`
	Message string         `json:"message"`
	File    string         `json:"file,omitempty"`
	Line    int            `json:"line,omitempty"`
	Column  int            `json:"column,omitempty"`
}

func (d Diagnostic) String() string {
	if d.File != "" && d.Line > 0 {
		return fmt.Sprintf("%s:%d: %s: %s", d.File, d.Line, d.Kind, d.Message)
	}
	return fmt.Sprintf("%s: %s", d.Kind, d.Message)
}

// ParseBuildOutput parses the stderr/stdout from a failed `go build` invocation
// into structured Diagnostic records. Lines that do not match the standard
// compiler diagnostic format are collected as error-kind diagnostics with the
// package header stripped.
func ParseBuildOutput(output string) []Diagnostic {
	var diags []Diagnostic
	lines := strings.Split(strings.TrimSpace(output), "\n")
	for _, line := range lines {
		if line == "" {
			continue
		}
		// Skip package header lines: "# <importpath>"
		if strings.HasPrefix(line, "# ") {
			continue
		}
		if d, ok := parseCompilerLine(line); ok {
			diags = append(diags, d)
		} else {
			diags = append(diags, Diagnostic{Kind: DiagnosticKindError, Message: strings.TrimSpace(line)})
		}
	}
	return diags
}

// parseCompilerLine attempts to parse "file:line:col: message" or
// "file:line: message" format. Returns (diag, true) on success.
func parseCompilerLine(line string) (Diagnostic, bool) {
	// Minimum: "f:1: m"
	if len(line) < 5 {
		return Diagnostic{}, false
	}
	// Split on ": " to separate location from message.
	location, message, ok2 := strings.Cut(line, ": ")
	if !ok2 {
		return Diagnostic{}, false
	}
	message = strings.TrimSpace(message)

	// Count colons in the location part. Need at least file:line.
	parts := strings.Split(location, ":")
	if len(parts) < 2 {
		return Diagnostic{}, false
	}

	// Last segment that is numeric is the column; one before is the line.
	var file string
	var lineNum, col int

	switch {
	case len(parts) >= 3:
		// file:line:col or C:\path:line:col (Windows drive letter)
		// Try rightmost two segments as line+col first.
		colStr := parts[len(parts)-1]
		lineStr := parts[len(parts)-2]
		c, cErr := strconv.Atoi(colStr)
		l, lErr := strconv.Atoi(lineStr)
		if cErr == nil && lErr == nil {
			col = c
			lineNum = l
			file = strings.Join(parts[:len(parts)-2], ":")
		} else {
			// Fall back to last segment as line only.
			l2, l2Err := strconv.Atoi(parts[len(parts)-1])
			if l2Err != nil {
				return Diagnostic{}, false
			}
			lineNum = l2
			file = strings.Join(parts[:len(parts)-1], ":")
		}
	case len(parts) == 2:
		l, lErr := strconv.Atoi(parts[1])
		if lErr != nil {
			return Diagnostic{}, false
		}
		lineNum = l
		file = parts[0]
	}

	if file == "" || lineNum <= 0 {
		return Diagnostic{}, false
	}

	return Diagnostic{
		Kind:    DiagnosticKindError,
		Message: message,
		File:    file,
		Line:    lineNum,
		Column:  col,
	}, true
}
