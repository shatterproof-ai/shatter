// Repo-wide regression guard for str-zutu / str-qo1.15 / str-s4cg.
//
// Go's default `-buildvcs=auto` causes `go build` to fail with
//
//	error obtaining VCS status: exit status 128
//
// in any tree that is "git-shaped but unprobeable" — preview worktrees
// under /tmp, vendored snapshots, broken `.git` ancestors, etc. Every
// `go build` invocation in this repo (Go test code, Rust callers, Taskfile
// targets) must pass `-buildvcs=false` so the toolchain skips VCS stamping.
//
// This test scans the repo for `go build` invocations and fails if any of
// them are missing the flag. It catches regressions in shatter-go test
// helpers, Rust callers (shatter-cli, shatter-core), and the shatter-go
// Taskfile.
//
// Scope: which Go subcommands need the flag (str-s4cg audit, 2026-05).
//
// VCS stamping is invoked only by Go subcommands that produce an
// installable/saved binary — i.e. `go build` and `go install`. The
// surrounding subcommands (`go test`, `go run`, `go vet`) build internally
// but do NOT stamp the resulting artifact, so they do NOT panic on
// `-buildvcs=auto` even in partial-git contexts.
//
// Empirical verification (Go 1.x, 2026-05): running each verb against a
// minimal `package main` module in (a) /tmp with no `.git` ancestor and
// (b) a tree with a corrupt `.git` parent reproduces the panic only for
// `go build`. `go vet ./...`, `go run main.go`, and `go test ./...` all
// succeed without `-buildvcs=false`.
//
// Consequence for the meta-test: only `go build` (and `go install`, if
// it ever appears) is enforced here. The callsites flagged by str-s4cg
// — `go vet` and `go run` in shatter-go/instrument/instrument_test.go,
// `go test` in shatter-core/src/test_runner.rs — are deliberately left
// without the flag, and the scan is intentionally not extended to those
// verbs. This header is the single source of truth; do not add per-site
// pointer comments. If a future audit re-flags one of those subcommands,
// re-read this header rather than adding the flag defensively.
//
// Opt-out: a callsite may be excluded by adding the marker
// `buildvcs-meta-skip` on the same line (use `// buildvcs-meta-skip` for
// Go/Rust, `# buildvcs-meta-skip` for YAML/Taskfile). Use sparingly and
// only when the invocation is genuinely not a `go build` (e.g. a string
// containing the substring inside test data).
package main

import (
	"os"
	"path/filepath"
	"regexp"
	"runtime"
	"strings"
	"testing"
)

// repoRoot returns the shatter repo root by walking up from this file.
func repoRoot(t *testing.T) string {
	t.Helper()
	_, thisFile, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("runtime.Caller(0) failed")
	}
	// thisFile is .../shatter-go/buildvcs_meta_test.go; root is two up.
	return filepath.Dir(filepath.Dir(thisFile))
}

// scanRoots are the subtrees within the repo that may contain `go build`
// invocations. Anything outside these roots is not scanned (e.g. shatter-ts
// node_modules, shatter-rust target, beads .beads).
var scanRoots = []string{"shatter-go", "shatter-cli", "shatter-core"}

// skipDirs are directory basenames pruned during the walk.
var skipDirs = map[string]bool{
	"target":       true,
	"node_modules": true,
	".git":         true,
	"vendor":       true,
}

// optOutMarker excludes a single callsite line from enforcement.
const optOutMarker = "buildvcs-meta-skip"

// goBuildPattern matches a `go build` invocation in either Go source
// (`exec.Command("go", "build"`), Rust source (`["build"`, `.args(["build"`,
// or `Command::new("go")` blocks), or shell/Taskfile (`go build`).
//
// We split detection into language-specific regexps because the surrounding
// syntax differs and we want to grab the whole argv block to check for the
// flag.
var (
	// Go: exec.Command("go", "build" ...) - capture up to closing paren.
	goSrcCallPattern = regexp.MustCompile(`(?s)exec\.Command\("go"\s*,\s*"build"[^)]*\)`)
	// Rust: Command::new("go") followed by an args invocation. Args may be
	// built inline on the chain (.args(["build", ...])) or constructed
	// earlier in the surrounding function as a Vec/array passed by ref.
	// To catch both shapes we anchor on `Command::new("go")` and check a
	// surrounding proximity window for the `"build"` literal and the flag.
	rustGoNewPattern = regexp.MustCompile(`Command::new\("go"\)`)
	// Shell/Taskfile: a line containing `go build` not wrapped in quotes
	// (so we don't false-positive on the literal `"go build"` substrings
	// in error-message strings).
	shellGoBuildPattern = regexp.MustCompile(`(?m)^[^"#\n]*\bgo\s+build\b[^"\n]*$`)
)

// fileLanguage classifies a path so we apply the right pattern set.
func fileLanguage(path string) string {
	switch filepath.Ext(path) {
	case ".go":
		return "go"
	case ".rs":
		return "rust"
	case ".yml", ".yaml":
		return "yaml"
	}
	if strings.HasSuffix(filepath.Base(path), "Taskfile.yml") {
		return "yaml"
	}
	return ""
}

// hasFlag reports whether the captured invocation contains -buildvcs=false.
func hasFlag(invocation string) bool {
	return strings.Contains(invocation, "-buildvcs=false")
}

// buildLineStarts returns the byte offset at which each 1-indexed line
// begins. lineStarts[0] is always 0; lineStarts[N-1] is the start of
// line N. Used for line-window slicing in the Rust scanner.
func buildLineStarts(source string) []int {
	starts := []int{0}
	for i, ch := range source {
		if ch == '\n' {
			starts = append(starts, i+1)
		}
	}
	return starts
}

// findInvocations returns (invocation, line, ok-skipped) tuples for every
// `go build` callsite in the file's source.
type invocation struct {
	text string
	line int
}

func findInvocations(language, source string) []invocation {
	var hits []invocation
	switch language {
	case "go":
		for _, m := range goSrcCallPattern.FindAllStringIndex(source, -1) {
			hits = append(hits, invocation{
				text: source[m[0]:m[1]],
				line: 1 + strings.Count(source[:m[0]], "\n"),
			})
		}
	case "rust":
		// rustProximityLines bounds the window we examine around each
		// `Command::new("go")` site when checking for `"build"` and the
		// VCS flag. Args are sometimes built into a Vec a few lines above
		// the Command::new chain (see shatter-cli/build_frontend.rs).
		const rustProximityLines = 40
		lineStarts := buildLineStarts(source)
		for _, m := range rustGoNewPattern.FindAllStringIndex(source, -1) {
			anchorLine := 1 + strings.Count(source[:m[0]], "\n")
			startLine := anchorLine - rustProximityLines
			if startLine < 1 {
				startLine = 1
			}
			endLine := anchorLine + rustProximityLines
			if endLine > len(lineStarts) {
				endLine = len(lineStarts)
			}
			windowStart := lineStarts[startLine-1]
			windowEnd := len(source)
			if endLine < len(lineStarts) {
				windowEnd = lineStarts[endLine]
			}
			window := source[windowStart:windowEnd]
			// Filter to windows that actually issue `go build` — a
			// `Command::new("go")` invocation that runs `go env` or
			// `go mod tidy` (no `"build"` literal nearby) is not in
			// scope for this regression.
			if !strings.Contains(window, `"build"`) {
				continue
			}
			hits = append(hits, invocation{
				text: window,
				line: anchorLine,
			})
		}
	case "yaml":
		for _, m := range shellGoBuildPattern.FindAllStringIndex(source, -1) {
			line := source[m[0]:m[1]]
			hits = append(hits, invocation{
				text: line,
				line: 1 + strings.Count(source[:m[0]], "\n"),
			})
		}
	}
	return hits
}

// lineHasOptOut reports whether the source line at position contains the
// opt-out marker. We check the line containing `position` (a byte offset
// into source).
func lineHasOptOut(source string, lineStartOffset int) bool {
	end := strings.IndexByte(source[lineStartOffset:], '\n')
	var line string
	if end < 0 {
		line = source[lineStartOffset:]
	} else {
		line = source[lineStartOffset : lineStartOffset+end]
	}
	return strings.Contains(line, optOutMarker)
}

func TestNoGoBuildMissingBuildVCSFalse(t *testing.T) {
	root := repoRoot(t)

	type violation struct {
		path string
		line int
		text string
	}
	var violations []violation
	// Track other go subcommands (test/run/vet) for an informational log
	// only — fixing them is out of scope for str-zutu.
	type otherHit struct {
		path string
		line int
		text string
	}
	var otherSubcommands []otherHit
	otherCmdRe := regexp.MustCompile(`(?s)exec\.Command\("go"\s*,\s*"(test|run|vet)"[^)]*\)`)
	rustOtherRe := regexp.MustCompile(`(?s)Command::new\("go"\)[^;]*?\.args\(\[\s*"(test|run|vet)"`)

	for _, sub := range scanRoots {
		base := filepath.Join(root, sub)
		walkErr := filepath.WalkDir(base, func(path string, d os.DirEntry, err error) error {
			if err != nil {
				return err
			}
			if d.IsDir() {
				if skipDirs[d.Name()] {
					return filepath.SkipDir
				}
				return nil
			}
			lang := fileLanguage(path)
			if lang == "" {
				return nil
			}
			// Skip the meta-test itself — it contains the literal patterns
			// we are scanning for as part of its own regexp source.
			if filepath.Base(path) == "buildvcs_meta_test.go" {
				return nil
			}
			data, err := os.ReadFile(path)
			if err != nil {
				return err
			}
			source := string(data)
			rel, _ := filepath.Rel(root, path)

			for _, inv := range findInvocations(lang, source) {
				// Compute the start-of-line byte offset for the opt-out
				// check and for the user-facing line number.
				offset := 0
				lineCount := 1
				for i, ch := range source {
					if lineCount == inv.line {
						offset = i
						break
					}
					if ch == '\n' {
						lineCount++
					}
				}
				if lineHasOptOut(source, offset) {
					continue
				}
				if !hasFlag(inv.text) {
					violations = append(violations, violation{
						path: rel,
						line: inv.line,
						text: strings.TrimSpace(inv.text),
					})
				}
			}

			// Informational: collect `go test` / `go run` / `go vet` sites
			// for the test log. Not a failure.
			if lang == "go" {
				for _, m := range otherCmdRe.FindAllStringIndex(source, -1) {
					otherSubcommands = append(otherSubcommands, otherHit{
						path: rel,
						line: 1 + strings.Count(source[:m[0]], "\n"),
						text: strings.TrimSpace(source[m[0]:m[1]]),
					})
				}
			}
			if lang == "rust" {
				for _, m := range rustOtherRe.FindAllStringIndex(source, -1) {
					otherSubcommands = append(otherSubcommands, otherHit{
						path: rel,
						line: 1 + strings.Count(source[:m[0]], "\n"),
						text: strings.TrimSpace(source[m[0]:m[1]]),
					})
				}
			}
			return nil
		})
		if walkErr != nil {
			t.Fatalf("walk %s: %v", sub, walkErr)
		}
	}

	if len(otherSubcommands) > 0 {
		t.Logf("Informational: %d `go test`/`go run`/`go vet` callsite(s) detected. These are out of scope for str-zutu but may have similar VCS-stamping issues:", len(otherSubcommands))
		for _, h := range otherSubcommands {
			t.Logf("  %s:%d  %s", h.path, h.line, h.text)
		}
	}

	if len(violations) == 0 {
		return
	}
	t.Errorf("%d `go build` callsite(s) missing -buildvcs=false:", len(violations))
	for _, v := range violations {
		t.Errorf("  %s:%d\n    %s", v.path, v.line, v.text)
	}
	t.Errorf("\nFix: add `-buildvcs=false` to each invocation. Pattern established in shatter-go/setup/loader.go and shatter-go/launcher/launcher.go. To exempt a non-build callsite, add the marker `%s` on the same line.", optOutMarker)
}
