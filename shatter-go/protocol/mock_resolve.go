package protocol

import (
	"go/ast"
	"go/types"

	"golang.org/x/tools/go/ast/astutil"
	"golang.org/x/tools/go/packages"

	"github.com/shatter-dev/shatter/shatter-go/instrument"
)

// resolveMockSubstitutionScopes annotates each substitution with the set of
// enclosing functions where its QualifiedFunction provably resolves to an
// imported package call, using the loaded package's TypesInfo (str-c8djq
// review fix 1). This prevents a config mock for package function
// `auth.GetAccount` from rewriting a method call on a same-named local
// (`auth := newClient(); auth.GetAccount(id)`).
//
// When type information is unavailable the substitutions are returned with
// TypeResolved=false, and the rewriter falls back to scope-aware syntactic
// matching. A caller-provided logf (may be nil) receives a one-line summary so
// operators can see when the safer type-resolved path could not run.
func resolveMockSubstitutionScopes(
	pkg *packages.Package,
	subs []instrument.MockSubstitution,
	logf func(msg string, args ...any),
) []instrument.MockSubstitution {
	if len(subs) == 0 {
		return subs
	}
	if pkg == nil || pkg.TypesInfo == nil || len(pkg.Syntax) == 0 {
		if logf != nil {
			logf("mock substitution: no type info; using syntactic call-site matching",
				"mocks", len(subs))
		}
		return subs
	}

	// Index substitutions by qualified function for O(1) lookup, and prepare
	// per-sub allow-lists.
	idx := make(map[string]int, len(subs))
	for i := range subs {
		subs[i].TypeResolved = true
		subs[i].AllowedFuncs = make(map[string]bool)
		subs[i].AllowPackageScope = false
		idx[subs[i].QualifiedFunction] = i
	}

	for _, file := range pkg.Syntax {
		var funcStack []string
		current := func() string {
			if len(funcStack) == 0 {
				return ""
			}
			return funcStack[len(funcStack)-1]
		}
		pre := func(c *astutil.Cursor) bool {
			switch n := c.Node().(type) {
			case *ast.FuncDecl:
				funcStack = append(funcStack, instrument.FuncKeyForDecl(n))
			case *ast.FuncLit:
				funcStack = append(funcStack, current())
			case *ast.CallExpr:
				sel, ok := n.Fun.(*ast.SelectorExpr)
				if !ok {
					return true
				}
				ident, ok := sel.X.(*ast.Ident)
				if !ok {
					return true
				}
				si, ok := idx[ident.Name+"."+sel.Sel.Name]
				if !ok {
					return true
				}
				// The qualifier must resolve to an imported package, not a
				// local variable / field / parameter of the same name.
				if _, isPkg := pkg.TypesInfo.Uses[ident].(*types.PkgName); !isPkg {
					return true
				}
				if enc := current(); enc == "" {
					subs[si].AllowPackageScope = true
				} else {
					subs[si].AllowedFuncs[enc] = true
				}
			}
			return true
		}
		post := func(c *astutil.Cursor) bool {
			switch c.Node().(type) {
			case *ast.FuncDecl, *ast.FuncLit:
				if len(funcStack) > 0 {
					funcStack = funcStack[:len(funcStack)-1]
				}
			}
			return true
		}
		astutil.Apply(file, pre, post)
	}

	if logf != nil {
		for _, s := range subs {
			if len(s.AllowedFuncs) == 0 && !s.AllowPackageScope {
				logf("mock substitution: symbol not called as a package function; inactive",
					"symbol", s.QualifiedFunction)
			}
		}
	}
	return subs
}
