// Package config parses the per-project .shatter/config.yaml hint file.
// The loader covers the safety-policy section consumed by the policy gate
// (str-hy9b.G4) plus the wider hint_config_v1 surface (defaults, mocks,
// generators) consumed by the Go planner (str-hy9b.G3). Unknown keys are
// reported via File.Warnings rather than returned as errors so the loader
// remains forward-compatible with future hint sections.
package config

import (
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"

	"gopkg.in/yaml.v3"
)

// File is the parsed on-disk representation of a .shatter/config.yaml.
//
// Warnings collects non-fatal issues — unknown keys, malformed sections — so
// the caller can log them without failing the request. The slice is empty
// when the file is absent or fully recognized.
type File struct {
	Functions       map[string]FunctionConfig       `yaml:"functions"`
	GoRuntimeValues map[string]GoRuntimeValueConfig `yaml:"go_runtime_values,omitempty"`

	// Warnings is the human-readable list of non-fatal config issues
	// surfaced during Load. It is populated even when Functions parses
	// successfully so partial-configuration drift is visible.
	Warnings []string `yaml:"-"`
}

// GoRuntimeValueConfig is a user-supplied runtime value for an exact Go type.
// Expression is pasted as a Go expression at wrapper/planner call sites, and
// Imports lists unaliased import paths required by the expression.
type GoRuntimeValueConfig struct {
	Expression string   `yaml:"expression"`
	Imports    []string `yaml:"imports,omitempty"`
}

// FunctionConfig is the per-target entry. Only known sections are decoded;
// unrecognized keys are reported via File.Warnings for forward compatibility.
type FunctionConfig struct {
	// Policy carries the user-facing safety policy overrides (str-hy9b.G4).
	Policy *PolicyConfig `yaml:"policy,omitempty"`

	// Defaults supplies per-parameter literal overrides keyed by parameter
	// name. The planner consumes them as top-priority ValuePlans, taking
	// precedence over classifyParamFamily defaults (str-hy9b.G3 AC1).
	Defaults map[string]DefaultValue `yaml:"defaults,omitempty"`

	// Mocks supplies per-target mock substitutions keyed by qualified
	// function name (e.g. "fmt.Println"). The value is the Go source
	// expression a code generator pastes in place of the original call
	// (str-hy9b.G3 AC2).
	Mocks map[string]string `yaml:"mocks,omitempty"`

	// Generators names a runtime-value registry entry per parameter. The
	// planner consults the named generator before falling back to primitive
	// families (str-hy9b.G3 AC3). Keys are parameter names; values are the
	// Go-source type spelling registered with the planner's runtime-value
	// registry (e.g. "context.Context", "*bytes.Buffer").
	Generators map[string]string `yaml:"generators,omitempty"`
}

// PolicyConfig carries the user-facing safety policy overrides.
type PolicyConfig struct {
	// Allow is the list of side-effect class names the user has opted
	// into for this target (e.g. ["database", "network"]). Names are
	// validated against the protocol.SideEffectClass enum at evaluation
	// time; unknown strings are dropped with a warn-level log.
	Allow []string `yaml:"allow,omitempty"`
}

// DefaultValue is one entry of a defaults map. It pairs the JSON-encoded
// literal (suitable for embedding directly in a planner ValuePlan) with the
// Go type-hint string the planner uses to label the produced ValuePlan.
type DefaultValue struct {
	// JSON is the canonical JSON encoding of the YAML scalar. Empty when
	// the YAML node is null or empty.
	JSON json.RawMessage
	// TypeHint is the Go type spelling implied by the YAML scalar
	// ("string", "int", "float64", or "bool"). Empty when the literal
	// type cannot be inferred from the scalar (e.g. an explicit yaml null).
	TypeHint string
}

// yaml type hints emitted from YAML scalars. The strings match the planner's
// paramTypeHint* constants so a DefaultValue threads through PlanParam
// without further translation.
const (
	defaultTypeHintString  = "string"
	defaultTypeHintInt     = "int"
	defaultTypeHintFloat64 = "float64"
	defaultTypeHintBool    = "bool"
)

// UnmarshalYAML converts a YAML scalar into a JSON literal plus an inferred
// Go type-hint. Non-scalar YAML (mapping, sequence) is preserved as JSON but
// no type-hint is inferred — callers can still consume the literal via the
// hint mechanism if they pass an explicit TypeHint.
func (d *DefaultValue) UnmarshalYAML(node *yaml.Node) error {
	if node == nil {
		return nil
	}
	var generic any
	if err := node.Decode(&generic); err != nil {
		return fmt.Errorf("decode default value: %w", err)
	}
	encoded, err := json.Marshal(generic)
	if err != nil {
		return fmt.Errorf("encode default value as JSON: %w", err)
	}
	d.JSON = encoded
	d.TypeHint = inferTypeHint(generic)
	return nil
}

func inferTypeHint(v any) string {
	switch v.(type) {
	case string:
		return defaultTypeHintString
	case bool:
		return defaultTypeHintBool
	case int, int8, int16, int32, int64, uint, uint8, uint16, uint32, uint64:
		return defaultTypeHintInt
	case float32, float64:
		return defaultTypeHintFloat64
	}
	return ""
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
	parsed.Warnings = collectUnknownKeyWarnings(data, path)
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

// known keys recognized at the top of a config file and inside each
// function entry. Anything else is reported via File.Warnings.
var (
	knownTopLevelKeys = map[string]struct{}{
		"functions":         {},
		"go_runtime_values": {},
	}
	knownFunctionKeys = map[string]struct{}{
		"policy":     {},
		"defaults":   {},
		"mocks":      {},
		"generators": {},
	}
)

// collectUnknownKeyWarnings re-decodes data into a generic yaml.Node tree
// and emits a warning for every mapping key that is not in the known set.
// The original strict decode in Load already produced the typed File; this
// pass exists purely to surface unknown keys without failing.
func collectUnknownKeyWarnings(data []byte, path string) []string {
	var root yaml.Node
	if err := yaml.Unmarshal(data, &root); err != nil {
		return []string{fmt.Sprintf("config %s: parse failed: %v", path, err)}
	}
	doc := documentMapping(&root)
	if doc == nil {
		return nil
	}
	var warnings []string
	for k, v := range mappingPairs(doc) {
		if _, ok := knownTopLevelKeys[k]; !ok {
			warnings = append(warnings, fmt.Sprintf("config %s: ignoring unknown top-level key %q", path, k))
			continue
		}
		if k == "functions" && v != nil {
			warnings = append(warnings, collectFunctionWarnings(path, v)...)
		}
	}
	sort.Strings(warnings)
	return warnings
}

// collectFunctionWarnings walks each function entry mapping and reports
// unknown keys. The function map itself is keyed by user globs which are
// not validated here.
func collectFunctionWarnings(path string, functions *yaml.Node) []string {
	var warnings []string
	for pattern, entry := range mappingPairs(functions) {
		if entry == nil || entry.Kind != yaml.MappingNode {
			continue
		}
		for k := range mappingPairs(entry) {
			if _, ok := knownFunctionKeys[k]; ok {
				continue
			}
			warnings = append(warnings, fmt.Sprintf("config %s: function %q: ignoring unknown key %q", path, pattern, k))
		}
	}
	return warnings
}

// documentMapping returns the top-level mapping node of a parsed YAML
// document, or nil when the document does not start with a mapping.
func documentMapping(root *yaml.Node) *yaml.Node {
	if root == nil {
		return nil
	}
	if root.Kind == yaml.DocumentNode {
		if len(root.Content) == 0 {
			return nil
		}
		root = root.Content[0]
	}
	if root.Kind != yaml.MappingNode {
		return nil
	}
	return root
}

// mappingPairs iterates a YAML mapping node yielding (key, value) pairs in
// document order. Returns an iterable wrapper so callers can range over a
// flat Go map without losing key→value association.
func mappingPairs(node *yaml.Node) map[string]*yaml.Node {
	if node == nil || node.Kind != yaml.MappingNode {
		return nil
	}
	out := make(map[string]*yaml.Node, len(node.Content)/2)
	for i := 0; i+1 < len(node.Content); i += 2 {
		key := node.Content[i]
		value := node.Content[i+1]
		if key == nil || key.Kind != yaml.ScalarNode {
			continue
		}
		out[key.Value] = value
	}
	return out
}
