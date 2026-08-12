package protocol

import (
	"go/types"
	"reflect"
	"strings"
)

// structSynthMaxDepth bounds struct-schema synthesis recursion, mirroring
// MaxTypeInfoDepth's rationale: types deeper than this (or self-referential
// via slice/map/pointer indirection) stop expanding rather than spin.
const structSynthMaxDepth = MaxTypeInfoDepth

// synthesizeStructDocument walks t's schema (field names, tags, and types)
// and returns a structurally valid document a real json.Unmarshal/
// yaml.Unmarshal into t would accept — populated with representative
// per-field values rather than zero values, so decode-success branches that
// inspect field content are reachable without an operator-supplied seed
// (str-4q7bd). format selects the wire-name convention: "json" reads the
// `json` struct tag (falling back to the raw Go field name), "yaml" reads
// the `yaml` struct tag (falling back to the lowercased field name, matching
// gopkg.in/yaml.v3's untagged-field default).
//
// Reports ok=false when t does not resolve to a struct (directly or through
// pointer/named indirection) — callers should skip synthesis for that
// parameter rather than emit a non-object document.
func synthesizeStructDocument(t types.Type, format string) (map[string]any, bool) {
	val, ok := synthesizeValue(t, format, structSynthMaxDepth, make(map[types.Type]bool))
	if !ok {
		return nil, false
	}
	m, ok := val.(map[string]any)
	return m, ok
}

// synthesizeValue returns a representative value for t, or ok=false when t
// has no safe synthesis (an interface, channel, func, or a named type with a
// custom text/binary marshaler this synthesizer does not model — e.g.
// time.Time — where an object-shaped guess would fail decode).
func synthesizeValue(t types.Type, format string, depth int, visited map[types.Type]bool) (any, bool) {
	if t == nil {
		return nil, false
	}
	if ptr, ok := t.(*types.Pointer); ok {
		// JSON/YAML have no pointer concept; populate through the pointer
		// rather than emitting null, so nil-check branches past the decode
		// are reachable too.
		return synthesizeValue(ptr.Elem(), format, depth, visited)
	}
	if named, ok := t.(*types.Named); ok {
		// Well-known stdlib types with custom (Un)marshalers (time.Time,
		// url.URL, regexp.Regexp, ...) need their own text-encoding formula
		// this synthesizer does not implement; guessing an object shape for
		// them would fail decode, so the field is omitted instead (decode
		// still succeeds, field stays zero-value). Follow-up work, not a
		// safety issue: str-4q7bd scope excludes this class of type.
		if complexKindFromNamed(named) != "" {
			return nil, false
		}
		// str-pjlc1 enum domains: seed the first constant so validating
		// decoders (UnmarshalJSON/IsValid) accept the value instead of a
		// generic string/int guess that is likely rejected.
		if values, _, ok := enumValuesFromNamed(named); ok && len(values) > 0 {
			return values[0], true
		}
	}
	if depth <= 0 {
		return nil, false
	}
	if visited[t] {
		// Self-referential type (e.g. type Node struct{ Children []*Node }):
		// stop expanding this occurrence rather than recurse forever.
		return nil, false
	}
	visited[t] = true
	defer delete(visited, t)

	switch u := t.Underlying().(type) {
	case *types.Basic:
		return basicSynthValue(u)
	case *types.Slice:
		return synthesizeSliceLike(u.Elem(), format, depth, visited)
	case *types.Array:
		return synthesizeSliceLike(u.Elem(), format, depth, visited)
	case *types.Map:
		return synthesizeMap(u, format, depth, visited)
	case *types.Struct:
		return synthesizeStructFields(u, format, depth-1, visited), true
	default:
		// Interface, chan, func, tuple, signature: no safe synthesis.
		return nil, false
	}
}

func synthesizeSliceLike(elemType types.Type, format string, depth int, visited map[types.Type]bool) (any, bool) {
	elem, ok := synthesizeValue(elemType, format, depth-1, visited)
	if !ok {
		// An unsynthesizable element still yields a valid (empty) array
		// rather than failing the whole document.
		return []any{}, true
	}
	return []any{elem}, true
}

func synthesizeMap(m *types.Map, format string, depth int, visited map[types.Type]bool) (any, bool) {
	keyBasic, ok := m.Key().Underlying().(*types.Basic)
	if !ok || keyBasic.Info()&types.IsString == 0 {
		// Only string-keyed maps are in scope for this slice — JSON object
		// keys are strings, and a non-string Go map key needs a marshal
		// convention (MarshalText, integer stringification, ...) this
		// synthesizer does not implement yet.
		return nil, false
	}
	val, ok := synthesizeValue(m.Elem(), format, depth-1, visited)
	if !ok {
		return map[string]any{}, true
	}
	return map[string]any{"key1": val}, true
}

func synthesizeStructFields(s *types.Struct, format string, depth int, visited map[types.Type]bool) map[string]any {
	doc := make(map[string]any)
	for i := 0; i < s.NumFields(); i++ {
		f := s.Field(i)
		name, ok := wireFieldName(f, s.Tag(i), format)
		if !ok {
			continue
		}
		val, ok := synthesizeValue(f.Type(), format, depth, visited)
		if !ok {
			continue
		}
		doc[name] = val
	}
	return doc
}

// wireFieldName resolves the document key a real decoder would bind field f
// to. Reports ok=false for unexported fields, embedded fields (promotion
// flattening is a follow-up, not modeled here), and fields explicitly
// excluded via `json:"-"` / `yaml:"-"`.
func wireFieldName(f *types.Var, structTag string, format string) (string, bool) {
	if !f.Exported() || f.Embedded() {
		return "", false
	}
	tagKey := "json"
	if format == "yaml" {
		tagKey = "yaml"
	}
	if tagVal, hasTag := reflect.StructTag(structTag).Lookup(tagKey); hasTag {
		name, _, _ := strings.Cut(tagVal, ",")
		if name == "-" {
			return "", false
		}
		if name != "" {
			return name, true
		}
	}
	if format == "yaml" {
		// gopkg.in/yaml.v3 default (no tag): lowercased Go field name.
		return strings.ToLower(f.Name()), true
	}
	return f.Name(), true
}

// basicSynthValue returns a representative literal for a basic Go kind, or
// ok=false for kinds with no natural JSON/YAML scalar representation
// (complex64/128, unsafe.Pointer, and the untyped/invalid kinds that never
// appear on a resolved struct field).
func basicSynthValue(b *types.Basic) (any, bool) {
	info := b.Info()
	switch {
	case info&types.IsString != 0:
		return "a", true
	case info&types.IsBoolean != 0:
		return true, true
	case info&types.IsInteger != 0:
		return 1, true
	case info&types.IsFloat != 0:
		return 1.5, true
	default:
		return nil, false
	}
}
