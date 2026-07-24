package wrapper

import (
	"go/ast"
	"go/types"
)

// MaxErrorSentinelsPerParam caps the number of mined errors.Is/errors.As
// sentinel targets per error parameter (str-kvzh7). Callers log truncation at
// warn level; the wrapper's baked table and the planner's candidate count both
// honour the same cap so their sentinel indices stay aligned.
const MaxErrorSentinelsPerParam = 16

// ErrorSentinel is one mined sentinel target for an error parameter: a
// Go-source expression referencing an exported package-level error variable
// (e.g. `nl.ErrQuestionRequired`), plus the import path needed to reference it
// from the generated wrapper file. ImportPath is empty when the sentinel is
// declared in the target's own package — the wrapper is generated in that same
// package, so such a sentinel is referenced by bare name.
type ErrorSentinel struct {
	// Expr is the Go-source expression the wrapper pastes into its baked
	// sentinel table (e.g. `nl.ErrQuestionRequired` or a bare `ErrLocal`).
	Expr string
	// ImportPath is the package import path Expr references, or "" when the
	// sentinel lives in the target's own package.
	ImportPath string
}

// MineErrorSentinels walks body for `errors.Is` / `errors.As` calls whose first
// argument is one of the named error parameters and whose second argument
// resolves to an exported package-level error variable. It returns, per
// parameter name, the ordered deduplicated sentinel targets (capped at
// MaxErrorSentinelsPerParam).
//
// pkgPath is the import path of the package being analysed; a sentinel declared
// in that package is emitted with an empty ImportPath and a bare-name Expr.
//
// The traversal is deterministic (ast.Inspect, source order) and pure over
// (body, info, pkgPath, paramNames) so the wrapper's baked table and the
// planner's candidate count derive identical sentinel indices from the same
// input. Returns nil when nothing is mined.
func MineErrorSentinels(
	body *ast.BlockStmt,
	info *types.Info,
	pkgPath string,
	errorParamNames map[string]bool,
) map[string][]ErrorSentinel {
	if body == nil || info == nil || len(errorParamNames) == 0 {
		return nil
	}

	out := make(map[string][]ErrorSentinel)
	seen := make(map[string]map[string]bool)
	add := func(param string, s ErrorSentinel) {
		if len(out[param]) >= MaxErrorSentinelsPerParam {
			return
		}
		if seen[param] == nil {
			seen[param] = make(map[string]bool)
		}
		if seen[param][s.Expr] {
			return
		}
		seen[param][s.Expr] = true
		out[param] = append(out[param], s)
	}

	ast.Inspect(body, func(n ast.Node) bool {
		call, ok := n.(*ast.CallExpr)
		if !ok || len(call.Args) < 2 {
			return true
		}
		if !isErrorsIsOrAsCall(call.Fun, info) {
			return true
		}
		paramName, ok := errorParamArgName(call.Args[0], errorParamNames)
		if !ok {
			return true
		}
		sentinel, ok := sentinelFromArg(call.Args[1], info, pkgPath)
		if !ok {
			return true
		}
		add(paramName, sentinel)
		return true
	})

	if len(out) == 0 {
		return nil
	}
	return out
}

// isErrorsIsOrAsCall reports whether fun is a selector `errors.Is` or
// `errors.As` bound to the standard library "errors" package. It resolves the
// qualifier through TypesInfo so a shadowed local named `errors` cannot produce
// a false positive.
func isErrorsIsOrAsCall(fun ast.Expr, info *types.Info) bool {
	sel, ok := fun.(*ast.SelectorExpr)
	if !ok {
		return false
	}
	if sel.Sel.Name != "Is" && sel.Sel.Name != "As" {
		return false
	}
	ident, ok := sel.X.(*ast.Ident)
	if !ok {
		return false
	}
	pkgName, ok := info.Uses[ident].(*types.PkgName)
	if !ok || pkgName.Imported() == nil {
		return false
	}
	return pkgName.Imported().Path() == "errors"
}

// errorParamArgName returns the parameter name when arg is a bare identifier
// naming one of errorParamNames. errors.Is/As take the candidate error as their
// first argument, so a matching identifier keys the mined sentinels.
func errorParamArgName(arg ast.Expr, errorParamNames map[string]bool) (string, bool) {
	ident, ok := arg.(*ast.Ident)
	if !ok {
		return "", false
	}
	if !errorParamNames[ident.Name] {
		return "", false
	}
	return ident.Name, true
}

// sentinelFromArg resolves arg to an exported package-level error variable and
// returns its wrapper-referenceable expression. A leading address-of (the
// `&target` form errors.As takes) is unwrapped first. Non-sentinel second
// arguments — locals, function-call results, unexported vars, non-error vars —
// return ok=false so the miner never proposes a value the wrapper cannot
// reference or that would not satisfy sentinel identity.
func sentinelFromArg(arg ast.Expr, info *types.Info, pkgPath string) (ErrorSentinel, bool) {
	if unary, ok := arg.(*ast.UnaryExpr); ok && unary.Op.String() == "&" {
		arg = unary.X
	}

	var ident *ast.Ident
	switch e := arg.(type) {
	case *ast.Ident:
		ident = e
	case *ast.SelectorExpr:
		ident = e.Sel
	default:
		return ErrorSentinel{}, false
	}

	obj, ok := info.Uses[ident].(*types.Var)
	if !ok {
		return ErrorSentinel{}, false
	}
	pkg := obj.Pkg()
	if pkg == nil {
		return ErrorSentinel{}, false
	}
	// Package-level only: the variable's declaring scope must be its package
	// scope. Locals (errors.As `&pe` targets, block-scoped vars) are excluded.
	if obj.Parent() != pkg.Scope() {
		return ErrorSentinel{}, false
	}
	if !obj.Exported() {
		return ErrorSentinel{}, false
	}
	if !isErrorValued(obj.Type()) {
		return ErrorSentinel{}, false
	}

	if pkg.Path() == pkgPath {
		// Same package as the generated wrapper: reference by bare name.
		return ErrorSentinel{Expr: obj.Name()}, true
	}
	return ErrorSentinel{
		Expr:       pkg.Name() + "." + obj.Name(),
		ImportPath: pkg.Path(),
	}, true
}

// isErrorValued reports whether t is or implements the builtin error interface.
// Sentinel vars are typically declared with static type `error`
// (`var ErrX = errors.New(...)`), but a concrete pointer type implementing
// error is accepted too.
func isErrorValued(t types.Type) bool {
	if t == nil {
		return false
	}
	errIface, ok := types.Universe.Lookup("error").Type().Underlying().(*types.Interface)
	if !ok {
		return false
	}
	return types.Implements(t, errIface) || types.Identical(t.Underlying(), errIface)
}
