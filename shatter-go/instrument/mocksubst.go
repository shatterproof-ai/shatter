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
		key := normalizeMockSymbol(m.Symbol)
		if key == "" || seen[key] {
			continue
		}
		seen[key] = true
		subs = append(subs, MockSubstitution{
			QualifiedFunction: key,
			Expression:        m.Expression,
		})
	}
	return subs
}

// DedupeMocks collapses mocks that normalize to the same qualified function,
// preserving first-seen order. When both a wire mock (ReturnValues, empty
// Expression) and a config mock (Expression) target the same symbol, the
// expression-bearing entry wins — otherwise sanitizeMockName would collapse
// both into one ShatterMock_<name> and generateLoopMockFile would emit a
// duplicate function declaration that fails to compile (str-c8djq review
// fix 2). Config call-site substitution takes precedence over a wire
// return-value shim for the same symbol.
func DedupeMocks(mocks []MockConfig) []MockConfig {
	if len(mocks) < 2 {
		return mocks
	}
	pos := make(map[string]int, len(mocks))
	out := make([]MockConfig, 0, len(mocks))
	for _, m := range mocks {
		key := normalizeMockSymbol(m.Symbol)
		if key == "" {
			key = "\x00raw\x00" + m.Symbol
		}
		if i, seen := pos[key]; seen {
			// Upgrade to the expression-bearing entry (config wins).
			if strings.TrimSpace(out[i].Expression) == "" && strings.TrimSpace(m.Expression) != "" {
				out[i] = m
			}
			continue
		}
		pos[key] = len(out)
		out = append(out, m)
	}
	return out
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
	symbol = strings.TrimSpace(symbol)
	if symbol == "" {
		return ""
	}
	// Fold "module:Export" into "module.Export".
	if idx := strings.LastIndex(symbol, ":"); idx >= 0 {
		symbol = symbol[:idx] + "." + symbol[idx+1:]
	}
	// Reduce a dotted path to its final "qualifier.Func" pair. The qualifier
	// is the last path segment before the function name; anything earlier is
	// an import-path prefix that never appears at the source call site.
	parts := strings.Split(symbol, ".")
	if len(parts) < 2 {
		return ""
	}
	qualifier := parts[len(parts)-2]
	fn := parts[len(parts)-1]
	// A colon-form import path may leave a slash-qualified segment
	// ("example.com/pkg"); the source call site uses only the final path
	// element as the package qualifier.
	if idx := strings.LastIndex(qualifier, "/"); idx >= 0 {
		qualifier = qualifier[idx+1:]
	}
	// Both halves must be plausible Go identifiers: a selector call site can
	// only ever match identifier.identifier, so anything else (stray colons,
	// spaces, empty segments from malformed config) is inert junk — reject it
	// here rather than letting it pollute dedupe keys and rewrite maps.
	if !isGoIdentifier(qualifier) || !isGoIdentifier(fn) {
		return ""
	}
	return qualifier + "." + fn
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

	byKey := make(map[string]MockSubstitution, len(subs))
	var parseErrs []string
	for _, s := range subs {
		if s.QualifiedFunction == "" || strings.TrimSpace(s.Expression) == "" {
			continue
		}
		if _, err := parser.ParseExpr(s.Expression); err != nil {
			parseErrs = append(parseErrs, fmt.Sprintf("%s: %v", s.QualifiedFunction, err))
			continue
		}
		byKey[s.QualifiedFunction] = s
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
			sub, ok := byKey[ident.Name+"."+sel.Sel.Name]
			if !ok {
				return true
			}
			if !mockCallSiteAllowed(sub, currentFunc(), ident.Name, imports, boundByFunc) {
				return true
			}
			// Parse a fresh expression per call site so replaced nodes never
			// share AST identity (which would confuse the printer).
			repl, err := parser.ParseExpr(sub.Expression)
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
