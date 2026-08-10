package instrument

import (
	"go/ast"

	"golang.org/x/tools/go/ast/astutil"
)

// QualifiedCallSite describes one `qualifier.Name(...)` call site discovered by
// WalkQualifiedCalls, together with the enclosing top-level function it appears
// in.
type QualifiedCallSite struct {
	// EnclosingFunc is the funcKey of the nearest enclosing top-level function
	// declaration (see FuncKeyForDecl), or "" for a call at package scope —
	// including calls inside a function literal in a package-level declaration,
	// which inherit the empty key.
	EnclosingFunc string
	// Qualifier is the identifier on the left of the selector ("auth" in
	// `auth.GetAccount(id)`).
	Qualifier string
	// FuncName is the selected name ("GetAccount").
	FuncName string
	// Call is the whole call expression.
	Call *ast.CallExpr
	// QualifierIdent is the qualifier identifier node, needed by callers that
	// consult type information (e.g. TypesInfo.Uses) to prove it names a
	// package.
	QualifierIdent *ast.Ident
	// Selector is the selector expression naming the function. For a generic
	// instantiation (`pkg.Func[T](...)`) this is the selector *inside* the
	// index expression, not Call.Fun.
	Selector *ast.SelectorExpr
	// Instantiated reports whether the callee was written as a generic
	// instantiation (`pkg.Func[T]` / `pkg.Func[T1, T2]`) rather than a plain
	// selector.
	Instantiated bool
}

// QualifiedName is the source-level "qualifier.Func" spelling of the call site,
// the key mock substitutions are matched on.
func (s QualifiedCallSite) QualifiedName() string { return s.Qualifier + "." + s.FuncName }

// WalkQualifiedCalls walks file and invokes visit for every call whose callee is
// written as `identifier.Name(...)`, including generic instantiations
// `identifier.Name[T](...)`. It is the single traversal shared by the mock
// call-site resolver (protocol/mock_resolve.go, which records per-function
// allow-lists from type information) and the rewriter (RewriteMockCallSites,
// which replaces allowed sites). Both passes must recognize exactly the same
// set of call sites — a resolver that records an allow-list the rewriter cannot
// reproduce silently disables mocking — so neither may reimplement this walk.
//
// If visit returns a non-nil expression, the whole call expression is replaced
// by it; returning nil leaves the tree unchanged. Replacement is exposed this
// way (rather than by handing out the astutil cursor) so read-only callers
// cannot accidentally mutate the AST.
//
// Enclosing-function keys follow FuncKeyForDecl. Function literals inherit the
// nearest named function's key; literals outside any FuncDecl report "".
func WalkQualifiedCalls(file *ast.File, visit func(site QualifiedCallSite) ast.Expr) {
	if file == nil || visit == nil {
		return
	}

	var funcStack []string
	currentFunc := func() string {
		if len(funcStack) == 0 {
			return ""
		}
		return funcStack[len(funcStack)-1]
	}

	pre := func(c *astutil.Cursor) bool {
		switch n := c.Node().(type) {
		case *ast.FuncDecl:
			funcStack = append(funcStack, funcKey(n))
		case *ast.FuncLit:
			funcStack = append(funcStack, currentFunc())
		case *ast.CallExpr:
			site, ok := qualifiedCallSite(n)
			if !ok {
				return true
			}
			site.EnclosingFunc = currentFunc()
			if repl := visit(site); repl != nil {
				c.Replace(repl)
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

// qualifiedCallSite decomposes a call expression whose callee is spelled
// `identifier.Name` or `identifier.Name[TypeArgs...]`.
//
// The generic form parses as *ast.IndexExpr (one type argument) or
// *ast.IndexListExpr (several) wrapping the selector, so both are unwrapped.
// Syntax alone cannot distinguish `pkg.Func[int](x)` from indexing a
// package-level container of funcs (`pkg.Handlers[i](x)`); both are reported,
// and callers with type information (the resolver) or a shadow check (the
// rewriter) apply their own guards. That is the pre-existing model for plain
// selectors too.
func qualifiedCallSite(call *ast.CallExpr) (QualifiedCallSite, bool) {
	callee := call.Fun
	instantiated := false
	switch fun := callee.(type) {
	case *ast.IndexExpr:
		callee = fun.X
		instantiated = true
	case *ast.IndexListExpr:
		callee = fun.X
		instantiated = true
	}
	sel, ok := callee.(*ast.SelectorExpr)
	if !ok {
		return QualifiedCallSite{}, false
	}
	ident, ok := sel.X.(*ast.Ident)
	if !ok {
		return QualifiedCallSite{}, false
	}
	return QualifiedCallSite{
		Qualifier:      ident.Name,
		FuncName:       sel.Sel.Name,
		Call:           call,
		QualifierIdent: ident,
		Selector:       sel,
		Instantiated:   instantiated,
	}, true
}
