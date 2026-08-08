package protocol

import (
	"go/ast"
	"go/types"
	"sort"
	"strings"

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

	matched := make([]bool, len(subs))
	// basePaths records, per bare-mock base qualifier, the distinct import paths
	// it matched, so an ambiguous base name (two packages, same base) warns.
	basePaths := map[string]map[string]bool{}

	// The traversal (call-site recognition, enclosing-function keys) is shared
	// with instrument.RewriteMockCallSites via WalkQualifiedCalls: the
	// allow-lists recorded here are only meaningful if the rewriter recognizes
	// exactly the same call sites. Returning nil keeps this pass read-only.
	for _, file := range pkg.Syntax {
		instrument.WalkQualifiedCalls(file, func(site instrument.QualifiedCallSite) ast.Expr {
			// The qualifier must resolve to an imported package, not a
			// local variable / field / parameter of the same name.
			pkgName, isPkg := pkg.TypesInfo.Uses[site.QualifierIdent].(*types.PkgName)
			if !isPkg {
				return nil
			}
			// Match on resolved package IDENTITY, not the source spelling: an
			// aliased import (`import a2 "example.com/auth"`) is the same
			// package as the config's "auth.GetAccount", and two packages
			// sharing a base name are not the same package despite sharing a
			// spelling (str-djcv2).
			resolvedPath := pkgName.Imported().Path()
			resolvedBase := pkgName.Imported().Name()
			for i := range subs {
				base, fn := mockSymbolParts(subs[i])
				if fn != site.FuncName {
					continue
				}
				if subs[i].ImportPath != "" {
					if resolvedPath != subs[i].ImportPath {
						continue
					}
				} else if resolvedBase != base {
					continue
				}
				matched[i] = true
				// Key by the spelling actually used at this call site so the
				// rewriter — which matches on spelling — can find it.
				rs := obtain(site.Qualifier+"."+site.FuncName, subs[i].Expression)
				if site.EnclosingFunc == "" {
					rs.allowPkg = true
				} else {
					rs.allowed[site.EnclosingFunc] = true
				}
				if subs[i].ImportPath == "" {
					if basePaths[base] == nil {
						basePaths[base] = map[string]bool{}
					}
					basePaths[base][resolvedPath] = true
				}
			}
			return nil
		})
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

// mockSymbolParts splits a substitution into the base package qualifier and the
// function name used for identity matching.
//
// MockSubstitutionsFromConfigs populates BaseQualifier, but MockSubstitution is
// a plain struct that other producers (and tests) construct with only
// QualifiedFunction set. Deriving the parts from QualifiedFunction when
// BaseQualifier is empty keeps those substitutions matching instead of silently
// resolving to nothing — the failure mode would be an empty allow-list, which
// the rewriter reads as "rewrite nowhere" and mocks would quietly stop applying.
func mockSymbolParts(s instrument.MockSubstitution) (base, fn string) {
	if s.BaseQualifier != "" {
		if len(s.BaseQualifier) < len(s.QualifiedFunction) {
			return s.BaseQualifier, s.QualifiedFunction[len(s.BaseQualifier)+1:]
		}
		return s.BaseQualifier, ""
	}
	dot := strings.LastIndex(s.QualifiedFunction, ".")
	if dot < 0 {
		return "", ""
	}
	return s.QualifiedFunction[:dot], s.QualifiedFunction[dot+1:]
}

// sortedStrings returns the set's members in sorted order, for stable logging.
func sortedStrings(set map[string]bool) []string {
	out := make([]string, 0, len(set))
	for s := range set {
		out = append(out, s)
	}
	sort.Strings(out)
	return out
}
