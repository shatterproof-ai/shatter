package instrument

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/printer"
	"go/token"
	"os"
	"strings"
	"unicode"

	"golang.org/x/tools/go/ast/astutil"
)

// MockSubstitution describes a single execute-time call-site replacement:
// every genuine package-qualified call to QualifiedFunction (e.g.
// "auth.GetAccount") is replaced by the Go source Expression. This is the
// runtime half of the hint_config_v1 `.shatter/config.yaml` `mocks` contract
// (str-c8djq).
//
// Matching must not fire on a method call on a same-named local
// (`auth := newClient(); auth.GetAccount(id)`). Two mechanisms guard against
// that:
//
//   - Type-resolved (preferred): when the loaded package's TypesInfo is
//     available at build time, AllowedFuncs enumerates the qualified names of
//     the enclosing functions where the qualifier provably resolves to an
//     imported package (via *types.PkgName). AllowPackageScope covers
//     package-level initializer calls. TypeResolved is set true so the
//     rewriter trusts the allow-list (an empty allow-list then means "rewrite
//     nowhere").
//   - Syntactic fallback: when TypeResolved is false (no type info), the
//     rewriter falls back to scope-aware syntactic matching — it rewrites a
//     call only when the qualifier is an imported package in that file and is
//     not shadowed by a local binding in the enclosing function.
type MockSubstitution struct {
	// QualifiedFunction is the source-level call spelling: the local package
	// qualifier followed by the function name, e.g. "auth.GetAccount".
	QualifiedFunction string
	// Expression is the Go source expression pasted in place of the whole
	// call. It must reference only packages already imported by the target
	// file (call-site substitution does not add imports).
	Expression string
	// AllowedFuncs, when TypeResolved is true, is the set of enclosing
	// function keys (funcKey) where a call to QualifiedFunction provably
	// targets the imported package. nil/empty with TypeResolved true means the
	// symbol is never a genuine package call and is rewritten nowhere.
	AllowedFuncs map[string]bool
	// AllowPackageScope, when TypeResolved is true, permits rewriting a
	// matching call that appears at package scope (e.g. a package-level var
	// initializer) rather than inside a function.
	AllowPackageScope bool
	// TypeResolved reports whether AllowedFuncs/AllowPackageScope were computed
	// from real type information. False selects the syntactic fallback.
	TypeResolved bool
	// ImportPath, when non-empty, is the resolved package import path this
	// substitution targets. It is set for path-qualified config spellings
	// ("example.com/pkg.Func") and lets type resolution match call sites by
	// package identity rather than the ambiguous base qualifier — so two
	// packages sharing a base name ("a/util.Do" vs "b/util.Do") no longer
	// collide, and an aliased import ("import a2 example.com/auth") still
	// matches. Empty for bare spellings.
	ImportPath string
	// BaseQualifier is the final package qualifier / base name (e.g. "auth"),
	// used to match a bare config spelling against a call site's resolved
	// package when no import path was supplied.
	BaseQualifier string
}

// parsedMockSymbol is the package-identity decomposition of a mock symbol.
// A mock symbol names a package-level function in one of several spellings:
//
//   - bare              "auth.GetAccount"              (Base="auth")
//   - path-qualified    "example.com/auth.GetAccount"  (ImportPath set)
//   - wire colon form   "example.com/auth:GetAccount"  (ImportPath set)
//   - wire colon bare   "auth:GetAccount"              (Base="auth")
//
// Base is always the final package qualifier — the alias-free source spelling
// a non-aliased import would use at the call site. ImportPath is non-empty only
// when the symbol carried a full slash-bearing import path, in which case
// matching and dedupe can key on package identity instead of the ambiguous
// base name.
type parsedMockSymbol struct {
	ImportPath string // full import path when path-qualified, else ""
	Base       string // final package qualifier / base name
	Func       string // function name
}

// identityKey is a dedupe/lookup key that collapses two symbols only when they
// provably name the same package function: path-qualified symbols key on the
// exact import path; bare symbols key on the base qualifier. A bare and a
// path-qualified spelling therefore never collapse — without type information
// the two cannot be proven identical, so collapsing them risks dropping a
// distinct mock (str-djcv2).
func (p parsedMockSymbol) identityKey() string {
	if p.ImportPath != "" {
		return "P\x00" + p.ImportPath + "\x00" + p.Func
	}
	return "B\x00" + p.Base + "\x00" + p.Func
}

// parseMockSymbol decomposes a mock symbol into its package identity. It
// returns false for spellings that cannot name a package-qualified call site
// (missing function, non-identifier base/function, empty segments).
func parseMockSymbol(symbol string) (parsedMockSymbol, bool) {
	symbol = strings.TrimSpace(symbol)
	if symbol == "" {
		return parsedMockSymbol{}, false
	}
	// Fold "path:Export" into "path.Export" so wire and config spellings funnel
	// through a single splitter.
	if idx := strings.LastIndex(symbol, ":"); idx >= 0 {
		symbol = symbol[:idx] + "." + symbol[idx+1:]
	}
	dot := strings.LastIndex(symbol, ".")
	if dot < 0 {
		return parsedMockSymbol{}, false
	}
	qualifier := symbol[:dot]
	fn := symbol[dot+1:]
	if qualifier == "" || fn == "" {
		return parsedMockSymbol{}, false
	}
	var importPath, base string
	if slash := strings.LastIndex(qualifier, "/"); slash >= 0 {
		// Slash-bearing qualifier: the whole thing is an import path; the base
		// is its final segment.
		importPath = qualifier
		base = qualifier[slash+1:]
	} else {
		// No slash: reduce a dotted qualifier ("nested.pkg.path") to its final
		// segment — only the last element appears at the source call site.
		base = qualifier
		if d := strings.LastIndex(base, "."); d >= 0 {
			base = base[d+1:]
		}
	}
	// A selector call site can only ever match identifier.identifier; reject
	// anything that could not appear there (stray punctuation, empty segments).
	if !isGoIdentifier(base) || !isGoIdentifier(fn) {
		return parsedMockSymbol{}, false
	}
	return parsedMockSymbol{ImportPath: importPath, Base: base, Func: fn}, true
}

// MockSubstitutionsFromConfigs extracts the expression-bearing subset of mocks
// as MockSubstitution entries. Wire mocks (ReturnValues only, empty Expression)
// are ignored — they flow through the ShatterMock shim path instead. When two
// mocks normalize to the same qualified function the first wins; callers that
// care about precedence should dedupe upstream (config Expression is deduped
// against wire mocks in the protocol handler).
//
// The returned substitutions are not type-resolved; callers with access to a
// loaded package should pass them through ResolveMockSubstitutionScopes.
func MockSubstitutionsFromConfigs(mocks []MockConfig) []MockSubstitution {
	var subs []MockSubstitution
	seen := make(map[string]bool)
	for _, m := range mocks {
		if strings.TrimSpace(m.Expression) == "" {
			continue
		}
		p, ok := parseMockSymbol(m.Symbol)
		if !ok {
			continue
		}
		key := p.identityKey()
		if seen[key] {
			continue
		}
		seen[key] = true
		subs = append(subs, MockSubstitution{
			// QualifiedFunction is the base source spelling used by the
			// syntactic fallback and as the seed spelling for type resolution;
			// the resolver may re-key an aliased call site to its actual local
			// spelling. ImportPath/BaseQualifier carry the package identity.
			QualifiedFunction: p.Base + "." + p.Func,
			ImportPath:        p.ImportPath,
			BaseQualifier:     p.Base,
			Expression:        m.Expression,
		})
	}
	return subs
}

// DedupeMocks collapses mocks that provably name the same package function,
// preserving first-seen order. Entries are keyed by package identity
// (parsedMockSymbol.identityKey): path-qualified symbols collapse only with an
// exact import-path match, bare symbols by base qualifier. Two packages sharing
// a base name ("a/util.Do" vs "b/util.Do") therefore no longer collide and
// silently drop one another (str-djcv2).
//
// When both a wire mock (ReturnValues, empty Expression) and a config mock
// (Expression) target the same symbol, they are MERGED rather than replaced:
// the Expression comes from the config entry while the wire entry's
// ReturnValues/ShouldTrackCalls/DefaultBehavior are preserved. Replacing
// wholesale used to discard the wire fields (str-djcv2). The single surviving
// entry keeps a unique sanitizeMockName so generateLoopMockFile cannot emit a
// duplicate ShatterMock_<name> declaration (str-c8djq review fix 2).
func DedupeMocks(mocks []MockConfig) []MockConfig {
	if len(mocks) < 2 {
		return mocks
	}
	pos := make(map[string]int, len(mocks))
	out := make([]MockConfig, 0, len(mocks))
	for _, m := range mocks {
		key := mockDedupeKey(m.Symbol)
		if i, seen := pos[key]; seen {
			out[i] = mergeMockConfigs(out[i], m)
			continue
		}
		pos[key] = len(out)
		out = append(out, m)
	}
	return out
}

// mockDedupeKey returns the package-identity dedupe key for a mock symbol,
// falling back to the raw symbol for spellings that cannot be parsed (so
// malformed symbols still collapse with themselves but never with a valid one).
func mockDedupeKey(symbol string) string {
	if p, ok := parseMockSymbol(symbol); ok {
		return p.identityKey()
	}
	return "\x00raw\x00" + symbol
}

// mergeMockConfigs combines two mocks proven to name the same symbol into one,
// preserving each half's contribution: Expression (config call-site
// substitution) and the wire shim fields (ReturnValues, ShouldTrackCalls,
// DefaultBehavior). The existing (first-seen) entry's Symbol spelling is kept
// for a stable sanitizeMockName. Non-empty / true values win field-by-field so
// the merge is order-independent for the fields either side supplies.
func mergeMockConfigs(existing, incoming MockConfig) MockConfig {
	merged := existing
	if strings.TrimSpace(merged.Expression) == "" && strings.TrimSpace(incoming.Expression) != "" {
		merged.Expression = incoming.Expression
	}
	if len(merged.ReturnValues) == 0 && len(incoming.ReturnValues) > 0 {
		merged.ReturnValues = incoming.ReturnValues
	}
	if strings.TrimSpace(merged.DefaultBehavior) == "" && strings.TrimSpace(incoming.DefaultBehavior) != "" {
		merged.DefaultBehavior = incoming.DefaultBehavior
	}
	if incoming.ShouldTrackCalls {
		merged.ShouldTrackCalls = true
	}
	return merged
}

// normalizeMockSymbol converts a mock symbol into the source-qualified
// "qualifier.Func" spelling used to match AST selector call sites.
//
// Config mocks already arrive dotted ("auth.GetAccount"). The legacy wire /
// discovery form uses a colon between module and export ("module:Export");
// the colon is normalized to a dot so both spellings match the same call
// syntax. Only the final qualifier segment is retained because a
// package-qualified call in Go source is always a single-identifier selector
// (import path segments never appear at the call site).
func normalizeMockSymbol(symbol string) string {
	p, ok := parseMockSymbol(symbol)
	if !ok {
		return ""
	}
	return p.Base + "." + p.Func
}

// isGoIdentifier reports whether s is a valid Go identifier.
func isGoIdentifier(s string) bool {
	if s == "" {
		return false
	}
	for i, r := range s {
		if unicode.IsLetter(r) || r == '_' || (i > 0 && unicode.IsDigit(r)) {
			continue
		}
		return false
	}
	return true
}

// FuncKeyForDecl is the exported form of funcKey, used by the protocol layer's
// type-resolution pass so both sides compute identical enclosing-function keys.
func FuncKeyForDecl(fd *ast.FuncDecl) string { return funcKey(fd) }

// funcKey returns a package-unique identity for a top-level function or method
// declaration: "Name" for a free function, "(recv).Name" for a method. Package
// funcs and (receiver, method) pairs are unique within a Go package, so the
// key correlates the same declaration between the original (type-resolved)
// AST and the instrumented AST, which preserves declaration names.
func funcKey(fd *ast.FuncDecl) string {
	name := fd.Name.Name
	if fd.Recv == nil || len(fd.Recv.List) == 0 {
		return name
	}
	var b strings.Builder
	_ = printer.Fprint(&b, token.NewFileSet(), fd.Recv.List[0].Type)
	return "(" + b.String() + ")." + name
}

// importedPackageNames returns the set of local package qualifiers a file
// imports (alias when present, else the final import-path segment). Dot and
// blank imports are excluded — neither introduces a usable selector qualifier.
func importedPackageNames(file *ast.File) map[string]bool {
	names := make(map[string]bool)
	for _, imp := range file.Imports {
		if name := ImportLocalName(imp); name != "" {
			names[name] = true
		}
	}
	return names
}

// ImportLocalName returns the name by which an import is referenced in its
// file: the alias when present, else the last path segment (the Go
// convention; packages whose declared name differs from their directory are
// the type-resolved pass's job). Blank and dot imports return "". Shared by
// the call-site rewriter and dependency discovery so the two sides can never
// disagree on which import a qualifier names (str-c8djq review).
func ImportLocalName(imp *ast.ImportSpec) string {
	if imp.Name != nil {
		switch imp.Name.Name {
		case "_", ".":
			return ""
		default:
			return imp.Name.Name
		}
	}
	path := strings.Trim(imp.Path.Value, `"`)
	if idx := strings.LastIndex(path, "/"); idx >= 0 {
		path = path[idx+1:]
	}
	return path
}

// collectBoundNames returns the set of identifier names bound as locals
// anywhere within a function declaration (parameters, receiver, named results,
// type params, `:=` targets, var/const declarations, range variables, and
// type-switch guards), including within nested function literals. It is used
// by the syntactic fallback to detect a qualifier shadowed by a local; it is
// deliberately conservative at function granularity — if a function binds
// `auth` anywhere, no `auth.X` call inside it is treated as a package call.
func collectBoundNames(fd *ast.FuncDecl) map[string]bool {
	bound := make(map[string]bool)
	add := func(idents ...*ast.Ident) {
		for _, id := range idents {
			if id != nil && id.Name != "_" {
				bound[id.Name] = true
			}
		}
	}
	addFieldList := func(fl *ast.FieldList) {
		if fl == nil {
			return
		}
		for _, f := range fl.List {
			add(f.Names...)
		}
	}
	addFieldList(fd.Recv)
	if fd.Type != nil {
		addFieldList(fd.Type.TypeParams)
		addFieldList(fd.Type.Params)
		addFieldList(fd.Type.Results)
	}
	ast.Inspect(fd, func(n ast.Node) bool {
		switch s := n.(type) {
		case *ast.AssignStmt:
			if s.Tok == token.DEFINE {
				for _, lhs := range s.Lhs {
					if id, ok := lhs.(*ast.Ident); ok {
						add(id)
					}
				}
			}
		case *ast.ValueSpec:
			add(s.Names...)
		case *ast.RangeStmt:
			if s.Tok == token.DEFINE {
				if id, ok := s.Key.(*ast.Ident); ok {
					add(id)
				}
				if id, ok := s.Value.(*ast.Ident); ok {
					add(id)
				}
			}
		case *ast.TypeSwitchStmt:
			if assign, ok := s.Assign.(*ast.AssignStmt); ok && len(assign.Lhs) == 1 {
				if id, ok := assign.Lhs[0].(*ast.Ident); ok {
					add(id)
				}
			}
		case *ast.FuncLit:
			if s.Type != nil {
				addFieldList(s.Type.Params)
				addFieldList(s.Type.Results)
			}
		}
		return true
	})
	return bound
}

// packageScopeFuncLitBoundNames collects names bound inside function literals
// that appear outside any FuncDecl (package-level var initializers and the
// like). Call sites inside such literals report an empty enclosing-function
// key, so their local bindings must feed the ""-scope shadow check — without
// it, `var H = func() int { dep := newClient(); return dep.Make() }` would
// have the local's method call rewritten (str-c8djq cross-review). Only
// FuncLit-internal bindings are collected: package-level identifiers resolve
// after file-block imports and are the type-resolved pass's job.
func packageScopeFuncLitBoundNames(file *ast.File) map[string]bool {
	bound := make(map[string]bool)
	for _, decl := range file.Decls {
		if _, isFunc := decl.(*ast.FuncDecl); isFunc {
			continue
		}
		ast.Inspect(decl, func(n ast.Node) bool {
			lit, ok := n.(*ast.FuncLit)
			if !ok {
				return true
			}
			// Wrap the literal in a synthetic FuncDecl so the existing
			// collector sees its params, results, and body bindings.
			synth := &ast.FuncDecl{Name: ast.NewIdent("_"), Type: lit.Type, Body: lit.Body}
			for name := range collectBoundNames(synth) {
				bound[name] = true
			}
			return false // collectBoundNames already walks nested literals
		})
	}
	return bound
}

// RewriteMockCallSites replaces, in file, every genuine package-qualified call
// matching a substitution's QualifiedFunction with that substitution's parsed
// Expression, and returns the number of call sites rewritten.
//
// Type-resolved substitutions (TypeResolved == true) rewrite only inside the
// enclosing functions listed in AllowedFuncs (plus package scope when
// AllowPackageScope is set). Non-type-resolved substitutions use scope-aware
// syntactic matching: a call is rewritten only when the qualifier is an
// imported package in this file and is not shadowed by a local binding in the
// enclosing function.
//
// Invalid substitution expressions are skipped (reported via the returned
// error) rather than aborting the rewrite of valid ones.
func RewriteMockCallSites(file *ast.File, subs []MockSubstitution) (int, error) {
	if file == nil || len(subs) == 0 {
		return 0, nil
	}

	// Multiple substitutions may share a local spelling: two packages with the
	// same base name, aliased to distinct import paths but both called
	// "util.Do" in different functions of this package, resolve to distinct
	// expressions with disjoint AllowedFuncs. The rewriter is position-blind and
	// keyed by spelling, so it holds every candidate per spelling and selects
	// the one whose allow-list covers the enclosing function (str-djcv2).
	byKey := make(map[string][]MockSubstitution, len(subs))
	var parseErrs []string
	for _, s := range subs {
		if s.QualifiedFunction == "" || strings.TrimSpace(s.Expression) == "" {
			continue
		}
		if _, err := parser.ParseExpr(s.Expression); err != nil {
			parseErrs = append(parseErrs, fmt.Sprintf("%s: %v", s.QualifiedFunction, err))
			continue
		}
		byKey[s.QualifiedFunction] = append(byKey[s.QualifiedFunction], s)
	}
	if len(byKey) == 0 {
		if len(parseErrs) > 0 {
			return 0, fmt.Errorf("mock substitution: %s", strings.Join(parseErrs, "; "))
		}
		return 0, nil
	}

	imports := importedPackageNames(file)

	// Precompute per-top-level-function bound names for the shadow checks.
	// The "" key holds bindings from package-scope function literals, whose
	// call sites also report an empty enclosing-function key.
	boundByFunc := make(map[string]map[string]bool)
	for _, decl := range file.Decls {
		if fd, ok := decl.(*ast.FuncDecl); ok {
			boundByFunc[funcKey(fd)] = collectBoundNames(fd)
		}
	}
	boundByFunc[""] = packageScopeFuncLitBoundNames(file)

	// Track the enclosing top-level function key as Apply descends. Function
	// literals inherit the nearest named function's key (they cannot be named
	// targets and their local bindings are already folded into that function's
	// bound-name set).
	var funcStack []string
	currentFunc := func() string {
		if len(funcStack) == 0 {
			return ""
		}
		return funcStack[len(funcStack)-1]
	}

	count := 0
	pre := func(c *astutil.Cursor) bool {
		switch n := c.Node().(type) {
		case *ast.FuncDecl:
			funcStack = append(funcStack, funcKey(n))
		case *ast.FuncLit:
			funcStack = append(funcStack, currentFunc())
		case *ast.CallExpr:
			sel, ok := n.Fun.(*ast.SelectorExpr)
			if !ok {
				return true
			}
			ident, ok := sel.X.(*ast.Ident)
			if !ok {
				return true
			}
			candidates, ok := byKey[ident.Name+"."+sel.Sel.Name]
			if !ok {
				return true
			}
			var chosen *MockSubstitution
			for i := range candidates {
				if mockCallSiteAllowed(candidates[i], currentFunc(), ident.Name, imports, boundByFunc) {
					chosen = &candidates[i]
					break
				}
			}
			if chosen == nil {
				return true
			}
			// Parse a fresh expression per call site so replaced nodes never
			// share AST identity (which would confuse the printer).
			repl, err := parser.ParseExpr(chosen.Expression)
			if err != nil {
				return true
			}
			c.Replace(repl)
			count++
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

	if len(parseErrs) > 0 {
		return count, fmt.Errorf("mock substitution: %s", strings.Join(parseErrs, "; "))
	}
	return count, nil
}

// mockCallSiteAllowed decides whether a matched call site should be rewritten.
func mockCallSiteAllowed(
	sub MockSubstitution,
	enclosingFunc, qualifier string,
	imports map[string]bool,
	boundByFunc map[string]map[string]bool,
) bool {
	if sub.TypeResolved {
		if enclosingFunc == "" {
			// Package scope: a package-level FuncLit binding the qualifier
			// makes ""-scope call sites ambiguous — skip them all.
			if bound := boundByFunc[""]; bound != nil && bound[qualifier] {
				return false
			}
			return sub.AllowPackageScope
		}
		if !sub.AllowedFuncs[enclosingFunc] {
			return false
		}
		// The allow-list is function-granular while this rewriter is
		// position-blind: in a function that both calls the package AND binds
		// a same-named local (`dep.Make(); dep := newClient(); dep.Make()`),
		// rewriting would also hit the local's method call. Skip the whole
		// function — conservative under-mocking, the real dependency runs.
		if bound := boundByFunc[enclosingFunc]; bound != nil && bound[qualifier] {
			return false
		}
		return true
	}
	// Syntactic fallback: the qualifier must be an imported package and must
	// not be shadowed by a local binding in the enclosing function.
	if !imports[qualifier] {
		return false
	}
	if bound := boundByFunc[enclosingFunc]; bound != nil && bound[qualifier] {
		return false
	}
	return true
}

// RewriteMockCallSitesInFile parses the Go source at path, applies
// RewriteMockCallSites, and rewrites the file in place when any call site
// changed. It is a no-op (returns 0) when no substitution applies. Comments
// are preserved. Used by the overlay build to substitute mocks into the
// already-instrumented target sources before `go build -overlay` compiles
// them.
func RewriteMockCallSitesInFile(path string, subs []MockSubstitution) (int, error) {
	if len(subs) == 0 {
		return 0, nil
	}
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, path, nil, parser.ParseComments)
	if err != nil {
		return 0, fmt.Errorf("mock substitution: parse %q: %w", path, err)
	}
	count, rewriteErr := RewriteMockCallSites(file, subs)
	if count == 0 {
		return 0, rewriteErr
	}
	var buf strings.Builder
	if err := printer.Fprint(&buf, fset, file); err != nil {
		return 0, fmt.Errorf("mock substitution: print %q: %w", path, err)
	}
	if err := os.WriteFile(path, []byte(buf.String()), 0o644); err != nil {
		return 0, fmt.Errorf("mock substitution: write %q: %w", path, err)
	}
	return count, rewriteErr
}
