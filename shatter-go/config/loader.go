// Package config parses the per-project .shatter/config.yaml hint file.
// Scope of this loader is narrow: only the safety-policy section needed
// by str-hy9b.G4. Broader hint-schema support (mocks, defaults, generators)
// is tracked under str-hy9b.G3 and should extend this package rather than
// replace it.
package config

import (
	"errors"
	"io/fs"
	"os"
	"path/filepath"

	"gopkg.in/yaml.v3"
)

// File is the parsed on-disk representation of a .shatter/config.yaml.
type File struct {
	Functions map[string]FunctionConfig `yaml:"functions"`
}

// FunctionConfig is the per-target entry. Only Policy is populated by
// this loader; unknown YAML keys are ignored for forward compatibility.
type FunctionConfig struct {
	Policy *PolicyConfig `yaml:"policy,omitempty"`
}

// PolicyConfig carries the user-facing safety policy overrides.
type PolicyConfig struct {
	// Allow is the list of side-effect class names the user has opted
	// into for this target (e.g. ["database", "network"]). Names are
	// validated against the protocol.SideEffectClass enum at evaluation
	// time; unknown strings are dropped with a warn-level log.
	Allow []string `yaml:"allow,omitempty"`
}

// Load locates the nearest .shatter/config.yaml by walking upward from
// the directory containing fromFile, parses it, and returns the result.
// A missing file returns a zero-value File with no error — callers
// should treat absence as "no overrides".
func Load(fromFile string) (File, error) {
	path, err := findConfigFile(fromFile)
	if err != nil {
		return File{}, err
	}
	if path == "" {
		return File{}, nil
	}
	data, err := os.ReadFile(path)
	if err != nil {
		return File{}, err
	}
	var parsed File
	if err := yaml.Unmarshal(data, &parsed); err != nil {
		return File{}, err
	}
	return parsed, nil
}

// findConfigFile walks upward from fromFile's directory looking for a
// .shatter/config.yaml. Returns "" if none is found before reaching the
// filesystem root.
func findConfigFile(fromFile string) (string, error) {
	abs, err := filepath.Abs(fromFile)
	if err != nil {
		return "", err
	}
	dir := filepath.Dir(abs)
	for {
		candidate := filepath.Join(dir, ".shatter", "config.yaml")
		info, err := os.Stat(candidate)
		if err == nil && !info.IsDir() {
			return candidate, nil
		}
		if err != nil && !errors.Is(err, fs.ErrNotExist) {
			return "", err
		}
		parent := filepath.Dir(dir)
		if parent == dir {
			return "", nil
		}
		dir = parent
	}
}

// MatchTarget returns the most specific FunctionConfig whose glob pattern
// matches relpath:function. Patterns use path.Match semantics on both
// sides of the colon (e.g. "*_test.go:*" or "models/user.go:Fetch*").
// Falls back to a zero-value FunctionConfig when no entry matches.
func (f File) MatchTarget(relpath, function string) FunctionConfig {
	const exactScore = 1000
	best := FunctionConfig{}
	bestScore := -1
	for pattern, entry := range f.Functions {
		score, matched := matchScore(pattern, relpath, function, exactScore)
		if !matched {
			continue
		}
		if score > bestScore {
			bestScore = score
			best = entry
		}
	}
	return best
}

// matchScore returns a match quality score for a pattern against a
// target identifier. Higher score = more specific. Exact literal matches
// on both sides score higher than glob matches.
func matchScore(pattern, relpath, function string, exactScore int) (int, bool) {
	fileGlob, funcGlob := splitPattern(pattern)
	if fileGlob == "" || funcGlob == "" {
		return 0, false
	}
	fileOK, fileExact := globMatch(fileGlob, relpath)
	if !fileOK {
		return 0, false
	}
	funcOK, funcExact := globMatch(funcGlob, function)
	if !funcOK {
		return 0, false
	}
	score := 0
	if fileExact {
		score += exactScore
	} else {
		score += len(fileGlob)
	}
	if funcExact {
		score += exactScore
	} else {
		score += len(funcGlob)
	}
	return score, true
}

func splitPattern(pattern string) (string, string) {
	for i := len(pattern) - 1; i >= 0; i-- {
		if pattern[i] == ':' {
			return pattern[:i], pattern[i+1:]
		}
	}
	return "", ""
}

// globMatch reports whether target matches pattern under path.Match
// semantics and whether the match was an exact literal (no wildcards).
func globMatch(pattern, target string) (bool, bool) {
	if pattern == target {
		return true, true
	}
	ok, err := filepath.Match(pattern, target)
	if err != nil {
		return false, false
	}
	return ok, false
}
