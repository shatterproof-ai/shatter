package planner

import (
	"fmt"
	"sort"
	"strings"

	"github.com/shatter-dev/shatter/shatter-go/protocol"
)

// DefaultMaxCompositeDepth caps recursive struct traversal in PlanComposite
// when CompositeOptions.MaxDepth is zero.
const DefaultMaxCompositeDepth = 3

// Field-value tokens used by the composite literal synthesizer.
// Kept here so the set of emitted zero-like values is a single source of
// truth shared across the primitive families.
const (
	compositeZeroString     = `""`
	compositeZeroInt        = "0"
	compositeZeroFloat      = "0"
	compositeZeroBool       = "false"
	compositeZeroNilPointer = "nil"
	compositeZeroByteSlice  = "nil"
)

// CompositeOptions bundles caller inputs for composite-literal synthesis.
type CompositeOptions struct {
	// MaxDepth caps recursive struct traversal. Zero means
	// DefaultMaxCompositeDepth. The top-level struct itself counts as
	// depth 1: MaxDepth=1 allows the top-level struct but forbids any
	// nested non-pointer struct field.
	MaxDepth int
}

// CompositePlan describes a synthesized Go composite-literal expression for
// a struct type.
type CompositePlan struct {
	// Expression is the Go source expression (e.g. `pkg.Req{Name: "", N: 0}`).
	Expression string
	// TypeHint is the Go source spelling of the top-level struct type
	// (e.g. "pkg.Req"). Matches the caller-supplied typeName.
	TypeHint string
	// Imports lists unique package import paths referenced by Expression,
	// sorted for determinism. Empty when the struct is package-local or
	// uses only builtin primitives.
	Imports []string
}

// PlanComposite synthesizes a Go composite-literal expression for a struct
// TypeInfo. typeName is the Go source spelling of the struct type (e.g.
// "pkg.Req"); pkgImport, when non-empty, is added to the returned Imports
// set so a wrapper can `import` the struct's defining package.
//
// Recursion is bounded by opts.MaxDepth. Within the bound, supported field
// shapes are:
//
//   - primitive families (string, int, float, bool, []byte) emitted as
//     zero-like source literals;
//   - nested non-pointer struct fields (emitted using Go's elided-type
//     composite literal, e.g. `Outer{Inner: {X: 0}}`) at a cost of one
//     depth level;
//   - pointer fields: always emitted as `nil`, regardless of remaining
//     depth, because Go requires a qualified type name in `&T{...}` form
//     that TypeInfo does not carry, and `nil` is a valid composite-literal
//     field value.
//
// Any field whose type resolves to an opaque, complex, unknown, union,
// function, channel, or map shape — including such a shape reached through
// a pointer — produces an UnsatisfiedRequirement for the whole composite
// (no partial values). Array fields are supported only when the element
// kind is byte-like (rendered as `nil`).
//
// The depth bound guarantees termination: a non-pointer nested struct that
// exhausts the bound produces an UnsatisfiedRequirement. Pointer fields
// terminate unconditionally via `nil`, so recursive types like
// `type Node struct{ Next *Node }` always produce a literal.
func PlanComposite(targetID, typeName, pkgImport string, t protocol.TypeInfo, opts CompositeOptions) (*CompositePlan, *protocol.UnsatisfiedRequirement) {
	maxDepth := opts.MaxDepth
	if maxDepth <= 0 {
		maxDepth = DefaultMaxCompositeDepth
	}
	imports := newCompositeImportSet()
	if pkgImport != "" {
		imports.add(pkgImport)
	}
	expr, err := synthesizeStructLiteral(typeName, t, maxDepth)
	if err != nil {
		return nil, &protocol.UnsatisfiedRequirement{
			Kind:     protocol.UnsatisfiedRequirementKindComplexType,
			TargetID: targetID,
			Detail:   err.Error(),
		}
	}
	return &CompositePlan{
		Expression: expr,
		TypeHint:   typeName,
		Imports:    imports.sorted(),
	}, nil
}

// synthesizeStructLiteral builds a typed composite-literal expression
// (`typeName{F: v, ...}`) for a struct TypeInfo at the given remaining
// depth budget. Returns an error describing the first offending field when
// the struct cannot be synthesized.
func synthesizeStructLiteral(typeName string, t protocol.TypeInfo, depth int) (string, error) {
	body, err := synthesizeStructBody(t, typeName, depth)
	if err != nil {
		return "", err
	}
	return typeName + body, nil
}

// synthesizeStructBody emits the `{F: v, ...}` portion of a composite
// literal. When called for a top-level struct the caller prepends the
// qualified type name; when called for a nested field the caller relies on
// Go's elided-type composite literal and prepends nothing.
func synthesizeStructBody(t protocol.TypeInfo, context string, depth int) (string, error) {
	if t.Kind != "object" {
		return "", fmt.Errorf("type %s is not a struct (kind=%q)", labelOr(context, "struct"), t.Kind)
	}
	if depth <= 0 {
		return "", fmt.Errorf("composite recursion depth bound reached at %s", labelOr(context, "struct"))
	}

	var b strings.Builder
	b.WriteString("{")
	for i, f := range t.Fields {
		if i > 0 {
			b.WriteString(", ")
		}
		b.WriteString(f.Name)
		b.WriteString(": ")
		val, err := synthesizeFieldValue(f.Type, depth-1)
		if err != nil {
			return "", fmt.Errorf("field %q: %w", f.Name, err)
		}
		b.WriteString(val)
	}
	b.WriteString("}")
	return b.String(), nil
}

// synthesizeFieldValue emits a Go source expression for a single field
// TypeInfo. depth is the budget remaining for any further recursion the
// field's type would require (i.e. the caller has already debited one
// level for the enclosing struct).
func synthesizeFieldValue(t protocol.TypeInfo, depth int) (string, error) {
	switch t.Kind {
	case "str":
		return compositeZeroString, nil
	case "int":
		return compositeZeroInt, nil
	case "float":
		return compositeZeroFloat, nil
	case "bool":
		return compositeZeroBool, nil
	case "array":
		if t.Element != nil && t.Element.Kind == "int" {
			return compositeZeroByteSlice, nil
		}
		return "", fmt.Errorf("array-of-%s is not a supported composite field type", describeKind(t.Element))
	case "nullable":
		// Pointers are always emitted as nil: Go's `&T{...}` requires a
		// qualified type name that TypeInfo does not carry, and nil is a
		// valid composite-literal field value. Opaque pointees still fail,
		// preserving the "DB *sql.DB → unsatisfied" acceptance case.
		if t.Inner != nil {
			switch t.Inner.Kind {
			case "opaque", "complex", "unknown", "union":
				return "", fmt.Errorf("pointer to %s is not a supported composite field type", describeKind(t.Inner))
			}
		}
		return compositeZeroNilPointer, nil
	case "object":
		// Non-pointer nested struct: emit an elided-type composite literal
		// (Go allows `Outer{Inner: {X: 0}}` without naming Inner's type),
		// at a cost of one further depth level.
		return synthesizeStructBody(t, "nested struct", depth)
	case "opaque", "complex", "unknown", "union":
		return "", fmt.Errorf("type %s is not synthesizable", describeKind(&t))
	default:
		return "", fmt.Errorf("unsupported field kind %q", t.Kind)
	}
}

func labelOr(primary, fallback string) string {
	if primary != "" {
		return primary
	}
	return fallback
}

func describeKind(t *protocol.TypeInfo) string {
	if t == nil {
		return "unknown"
	}
	if t.Label != "" {
		return t.Label
	}
	if t.ComplexKind != "" {
		return t.ComplexKind
	}
	if t.Kind != "" {
		return t.Kind
	}
	return "unknown"
}

// compositeImportSet is a small de-duplicating set for import paths with a
// deterministic listing. The planner package keeps this private; callers
// consume CompositePlan.Imports which is already sorted.
type compositeImportSet struct {
	paths map[string]struct{}
}

func newCompositeImportSet() *compositeImportSet {
	return &compositeImportSet{paths: make(map[string]struct{})}
}

func (s *compositeImportSet) add(path string) {
	if path == "" {
		return
	}
	s.paths[path] = struct{}{}
}

func (s *compositeImportSet) sorted() []string {
	if len(s.paths) == 0 {
		return nil
	}
	out := make([]string, 0, len(s.paths))
	for p := range s.paths {
		out = append(out, p)
	}
	sort.Strings(out)
	return out
}
