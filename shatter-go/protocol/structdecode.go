package protocol

import (
	"encoding/json"
	"go/ast"
	"go/token"
	"go/types"
	"strings"
)

// structDecodeSite records one detected json.Unmarshal/yaml.Unmarshal call
// site within a target function body whose bytes/string source resolves to
// a function parameter and whose decode target resolves to a named struct
// type.
type structDecodeSite struct {
	Format     string // "json" or "yaml"
	TargetType types.Type
}

// structDecodeSeedsByParam mines json.Unmarshal/yaml.Unmarshal call sites in
// fn's body and, for each parameter that feeds the decoded bytes/string
// directly (or through one level of local-variable aliasing), synthesizes a
// structurally valid document from the decode target struct's own schema
// (str-4q7bd). Each parameter maps to a small ordered pool of candidate
// documents: the schema-derived document first (reaches the deepest
// decode-success branches), then an empty-object fallback (reaches
// decode-success branches gated only on zero-value defaults). Returns nil
// when no struct-decode call site resolves to a parameter.
//
// Detection intentionally covers only the direct case: the decoded bytes
// come straight from a parameter (or one `:=`/`=` local alias of one), and
// the decode target is a resolvable named struct — not a generic type
// parameter, not `interface{}`/`any`, and not reached only through a
// file-path read (os.ReadFile et al). Those cases are out of scope for this
// slice; see str-4q7bd's design note.
func structDecodeSeedsByParam(fn *ast.FuncDecl, info *types.Info, params []ParamInfo) map[string][]json.RawMessage {
	if fn == nil || fn.Body == nil {
		return nil
	}
	paramNames := paramNameSet(params)
	if len(paramNames) == 0 {
		return nil
	}
	sites := findStructDecodeSites(fn, info, paramNames)
	if len(sites) == 0 {
		return nil
	}
	out := make(map[string][]json.RawMessage, len(sites))
	for paramName, site := range sites {
		doc, ok := synthesizeStructDocument(site.TargetType, site.Format)
		if !ok {
			continue
		}
		raw, err := json.Marshal(doc)
		if err != nil {
			continue
		}
		out[paramName] = []json.RawMessage{raw, json.RawMessage(`{}`)}
	}
	if len(out) == 0 {
		return nil
	}
	return out
}

// findStructDecodeSites walks fn's body for json.Unmarshal/yaml.Unmarshal
// calls whose first argument resolves to a parameter name and whose second
// argument (`&target`) resolves to a named struct type. The first matching
// site wins when a parameter appears in more than one call.
func findStructDecodeSites(fn *ast.FuncDecl, info *types.Info, paramNames map[string]bool) map[string]structDecodeSite {
	out := make(map[string]structDecodeSite)
	ast.Inspect(fn.Body, func(n ast.Node) bool {
		call, ok := n.(*ast.CallExpr)
		if !ok || len(call.Args) != 2 {
			return true
		}
		sel, ok := call.Fun.(*ast.SelectorExpr)
		if !ok || sel.Sel.Name != "Unmarshal" {
			return true
		}
		pkgIdent, ok := sel.X.(*ast.Ident)
		if !ok {
			return true
		}
		format := unmarshalFormat(pkgIdent, info)
		if format == "" {
			return true
		}
		paramName := resolveBytesSourceParam(fn, call.Args[0], info, paramNames)
		if paramName == "" {
			return true
		}
		if _, exists := out[paramName]; exists {
			return true
		}
		targetType, ok := resolveUnmarshalTargetStructType(call.Args[1], info)
		if !ok {
			return true
		}
		out[paramName] = structDecodeSite{Format: format, TargetType: targetType}
		return true
	})
	return out
}

// unmarshalFormat reports "json" or "yaml" when pkgIdent (the package
// qualifier of a call like pkg.Unmarshal(...)) resolves to encoding/json or
// a gopkg.in/yaml.vN import; "" otherwise.
func unmarshalFormat(pkgIdent *ast.Ident, info *types.Info) string {
	if info == nil {
		return ""
	}
	obj := info.Uses[pkgIdent]
	pkgName, ok := obj.(*types.PkgName)
	if !ok {
		return ""
	}
	path := pkgName.Imported().Path()
	switch {
	case path == "encoding/json":
		return "json"
	case strings.HasPrefix(path, "gopkg.in/yaml."):
		return "yaml"
	default:
		return ""
	}
}

// resolveBytesSourceParam reports the parameter name feeding expr, either
// directly (expr is the parameter identifier, optionally wrapped in a
// []byte(...)/string(...) conversion) or through exactly one local
// `alias := param` / `alias = param` assignment found in fn's body.
func resolveBytesSourceParam(fn *ast.FuncDecl, expr ast.Expr, info *types.Info, paramNames map[string]bool) string {
	ident := unwrapToIdent(expr, info)
	if ident == nil {
		return ""
	}
	if paramNames[ident.Name] {
		return ident.Name
	}
	aliased := ""
	ast.Inspect(fn.Body, func(n ast.Node) bool {
		if aliased != "" {
			return false
		}
		assign, ok := n.(*ast.AssignStmt)
		if !ok || len(assign.Lhs) != len(assign.Rhs) {
			return true
		}
		for i, lhs := range assign.Lhs {
			lhsIdent, ok := lhs.(*ast.Ident)
			if !ok || lhsIdent.Name != ident.Name {
				continue
			}
			if rhsIdent := unwrapToIdent(assign.Rhs[i], info); rhsIdent != nil && paramNames[rhsIdent.Name] {
				aliased = rhsIdent.Name
			}
		}
		return true
	})
	return aliased
}

// unwrapToIdent peels off parens and single-argument type-conversion calls
// (e.g. []byte(x), string(x)) to find the underlying identifier. Returns nil
// for any other expression shape (field selectors, arbitrary function calls
// with side effects, indexing, ...) — those are out of scope for this slice.
// A CallExpr is only unwrapped when info confirms its Fun denotes a type (a
// conversion), not a function value: without that check, a transform like
// decrypt(data) would unwrap to the untransformed identifier "data" and the
// synthesized plaintext seed would never reach a real decode success, since
// it never survives the intervening transform.
func unwrapToIdent(expr ast.Expr, info *types.Info) *ast.Ident {
	for {
		switch e := expr.(type) {
		case *ast.Ident:
			return e
		case *ast.ParenExpr:
			expr = e.X
		case *ast.CallExpr:
			if len(e.Args) != 1 || info == nil {
				return nil
			}
			tv, ok := info.Types[e.Fun]
			if !ok || !tv.IsType() {
				return nil
			}
			expr = e.Args[0]
		default:
			return nil
		}
	}
}

// resolveUnmarshalTargetStructType reports the struct type addressed by
// expr, which must be a plain `&target` unary expression whose operand's
// type resolves (directly, or through named-type indirection) to a
// go/types.Struct. Any other shape (a pre-existing pointer variable passed
// without `&`, an interface{}/any target, a generic type parameter) reports
// ok=false — out of scope for this slice.
func resolveUnmarshalTargetStructType(expr ast.Expr, info *types.Info) (types.Type, bool) {
	if info == nil {
		return nil, false
	}
	unary, ok := expr.(*ast.UnaryExpr)
	if !ok || unary.Op != token.AND {
		return nil, false
	}
	t := info.TypeOf(unary.X)
	if t == nil {
		return nil, false
	}
	if _, ok := t.Underlying().(*types.Struct); ok {
		return t, true
	}
	return nil, false
}
