package protocol

import (
	"go/ast"
	"go/types"
	"sort"

	"golang.org/x/tools/go/ast/astutil"
	"golang.org/x/tools/go/packages"

	"github.com/shatter-dev/shatter/shatter-go/instrument"
)

// resolveMockSubstitutionScopes matches each config mock to the call sites
// where it provably targets an imported package, using the loaded package's
// TypesInfo, and emits type-resolved substitutions keyed by the actual local
// call-site spelling (str-c8djq review fix 1, generalized by str-djcv2).
//
// Matching keys on package identity, not source spelling:
//
//   - A path-qualified mock ("example.com/a/util.Do") matches only call sites
//     whose qualifier resolves to that exact import path — so two packages
//     sharing a base name no longer collide.
//   - A bare mock ("auth.GetAccount") matches call sites whose resolved package
//     base name equals the qualifier, including aliased imports
//     ("import a2 example.com/auth" → a2.GetAccount). When a bare qualifier
//     resolves to more than one import path a warning is logged and the mock is
//     applied to every match (documented ambiguity resolution).
//
// This also guards against rewriting a method call on a same-named local
// (`auth := newClient(); auth.GetAccount(id)`): a local does not resolve to a
// *types.PkgName, so it never contributes an allow-list entry.
//
// When type information is unavailable the substitutions are returned unchanged
// (TypeResolved=false) and the rewriter falls back to scope-aware syntactic
// matching. A caller-provided logf (may be nil) receives one-line summaries.
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

	// resolvedSub accumulates one type-resolved substitution, keyed by the
	// actual local call-site spelling (which differs from the config spelling
	// for aliased imports) together with the expression, so two same-base mocks
	// that share a spelling but carry different expressions stay distinct.
	type resolvedSub struct {
		spelling string
		expr     string
		allowed  map[string]bool
		allowPkg bool
	}
	resolved := map[string]*resolvedSub{}
	var order []string
	obtain := func(spelling, expr string) *resolvedSub {
		key := spelling + "\x00" + expr
		rs, ok := resolved[key]
		if !ok {
			rs = &resolvedSub{spelling: spelling, expr: expr, allowed: map[string]bool{}}
			resolved[key] = rs
			order = append(order, key)
		}
		return rs
	}

	// funcName extracts the function half of a "base.Func" QualifiedFunction.
	// BaseQualifier is a single identifier, so exactly one dot separates them.
	funcName := func(s instrument.MockSubstitution) string {
		if len(s.BaseQualifier) < len(s.QualifiedFunction) {
			return s.QualifiedFunction[len(s.BaseQualifier)+1:]
		}
		return ""
	}

	matched := make([]bool, len(subs))
	// basePaths records, per bare-mock base qualifier, the distinct import paths
	// it matched, so an ambiguous base name (two packages, same base) warns.
	basePaths := map[string]map[string]bool{}

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
				// The qualifier must resolve to an imported package, not a
				// local variable / field / parameter of the same name.
				pkgName, isPkg := pkg.TypesInfo.Uses[ident].(*types.PkgName)
				if !isPkg {
					return true
				}
				resolvedPath := pkgName.Imported().Path()
				resolvedBase := pkgName.Imported().Name()
				fn := sel.Sel.Name
				for i := range subs {
					if funcName(subs[i]) != fn {
						continue
					}
					if subs[i].ImportPath != "" {
						if resolvedPath != subs[i].ImportPath {
							continue
						}
					} else if resolvedBase != subs[i].BaseQualifier {
						continue
					}
					matched[i] = true
					rs := obtain(ident.Name+"."+fn, subs[i].Expression)
					if enc := current(); enc == "" {
						rs.allowPkg = true
					} else {
						rs.allowed[enc] = true
					}
					if subs[i].ImportPath == "" {
						if basePaths[subs[i].BaseQualifier] == nil {
							basePaths[subs[i].BaseQualifier] = map[string]bool{}
						}
						basePaths[subs[i].BaseQualifier][resolvedPath] = true
					}
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

	// Every input mock that matched no call site still yields a type-resolved
	// entry with an empty allow-list ("rewrite nowhere"). This preserves the
	// build-side invariant that a non-empty resolved set means "resolution ran"
	// (see build/instrumented_overlay.go), never "the caller skipped it".
	for i := range subs {
		if matched[i] {
			continue
		}
		obtain(subs[i].QualifiedFunction, subs[i].Expression)
		if logf != nil {
			logf("mock substitution: symbol not called as a package function; inactive",
				"symbol", subs[i].QualifiedFunction)
		}
	}

	if logf != nil {
		for base, paths := range basePaths {
			if len(paths) > 1 {
				logf("mock substitution: bare qualifier matches multiple packages; substituting in all — use a path-qualified spelling to disambiguate",
					"qualifier", base, "paths", sortedStrings(paths))
			}
		}
	}

	out := make([]instrument.MockSubstitution, 0, len(order))
	for _, key := range order {
		rs := resolved[key]
		out = append(out, instrument.MockSubstitution{
			QualifiedFunction: rs.spelling,
			Expression:        rs.expr,
			AllowedFuncs:      rs.allowed,
			AllowPackageScope: rs.allowPkg,
			TypeResolved:      true,
		})
	}
	return out
}

func sortedStrings(set map[string]bool) []string {
	out := make([]string, 0, len(set))
	for s := range set {
		out = append(out, s)
	}
	sort.Strings(out)
	return out
}
