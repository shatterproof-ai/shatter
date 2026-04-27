package protocol

import (
	"go/ast"
	"go/token"
	"go/types"
	"strings"

	"golang.org/x/tools/go/packages"
)

// ScanConstructors scans a loaded package for functions that construct
// same-package types. A function qualifies when:
//   - its return signature is (T) or (T, error), where T is a same-package
//     named type (pointer or value), AND
//   - its name matches a constructor pattern (New*, MustNew*, Default*) OR
//     its body contains a composite-literal return (&T{...} or T{...}).
//
// Methods (functions with a receiver) are excluded; they already have a type.
func ScanConstructors(pkg *packages.Package) []ConstructorCandidate {
	if pkg == nil || pkg.TypesInfo == nil {
		return []ConstructorCandidate{}
	}
	var candidates []ConstructorCandidate
	for _, file := range pkg.Syntax {
		if file == nil {
			continue
		}
		for _, decl := range file.Decls {
			fn, ok := decl.(*ast.FuncDecl)
			if !ok || fn.Body == nil {
				continue
			}
			if fn.Recv != nil && len(fn.Recv.List) > 0 {
				continue // methods are not constructors
			}
			if c, ok := classifyConstructor(fn, pkg); ok {
				candidates = append(candidates, c)
			}
		}
	}
	if candidates == nil {
		return []ConstructorCandidate{}
	}
	return candidates
}

// classifyConstructor tests whether fn is a constructor candidate in pkg.
func classifyConstructor(fn *ast.FuncDecl, pkg *packages.Package) (ConstructorCandidate, bool) {
	targetType, returnsError, ok := returnsPackageType(fn, pkg)
	if !ok {
		return ConstructorCandidate{}, false
	}
	if !isConstructorName(fn.Name.Name) && !bodyReturnsComposite(fn) {
		return ConstructorCandidate{}, false
	}
	params := extractParamsWithContext(fn, pkg.TypesInfo, nil)
	return ConstructorCandidate{
		FuncName:     fn.Name.Name,
		TargetType:   targetType,
		Parameters:   params,
		ReturnsError: returnsError,
	}, true
}

// returnsPackageType reports whether fn returns a same-package named type as
// its first (and optionally second) result. Returns the bare type name,
// whether the second result is error, and whether the signature matches.
func returnsPackageType(fn *ast.FuncDecl, pkg *packages.Package) (typeName string, returnsError bool, ok bool) {
	results := fn.Type.Results
	if results == nil || len(results.List) == 0 {
		return "", false, false
	}

	// Flatten the result field list into individual expressions.
	var exprs []ast.Expr
	for _, field := range results.List {
		count := len(field.Names)
		if count == 0 {
			count = 1
		}
		for i := 0; i < count; i++ {
			exprs = append(exprs, field.Type)
		}
	}

	if len(exprs) == 0 || len(exprs) > 2 {
		return "", false, false
	}

	name, same := samePackageTypeName(exprs[0], pkg)
	if !same {
		return "", false, false
	}

	if len(exprs) == 2 {
		if !isErrorExpr(exprs[1], pkg.TypesInfo) {
			return "", false, false
		}
		return name, true, true
	}
	return name, false, true
}

// samePackageTypeName reports whether expr resolves to a named type whose
// defining package matches pkg. Pointer wrappers (*T) are unwrapped first.
// Returns the bare type name and true on success.
func samePackageTypeName(expr ast.Expr, pkg *packages.Package) (string, bool) {
	if star, ok := expr.(*ast.StarExpr); ok {
		expr = star.X
	}
	ident, ok := expr.(*ast.Ident)
	if !ok {
		return "", false
	}
	if pkg.TypesInfo == nil {
		return "", false
	}
	obj, ok := pkg.TypesInfo.Uses[ident]
	if !ok {
		// Fall back to Defs for locally defined identifiers in synthetic modules.
		obj, ok = pkg.TypesInfo.Defs[ident]
		if !ok {
			return "", false
		}
	}
	tn, ok := obj.(*types.TypeName)
	if !ok {
		return "", false
	}
	if tn.Pkg() == nil {
		return "", false
	}
	if tn.Pkg().Path() != pkg.PkgPath {
		return "", false
	}
	return tn.Name(), true
}

// isErrorExpr reports whether expr resolves to the built-in error interface.
func isErrorExpr(expr ast.Expr, info *types.Info) bool {
	if info == nil {
		return false
	}
	tv, ok := info.Types[expr]
	if !ok {
		// Fallback: unresolved identifier named "error".
		if ident, ok := expr.(*ast.Ident); ok {
			return ident.Name == "error"
		}
		return false
	}
	return tv.Type == types.Universe.Lookup("error").Type()
}

// isConstructorName reports whether name matches a standard Go constructor
// pattern: New, New<X>, MustNew<X>, or Default<X>.
func isConstructorName(name string) bool {
	return strings.HasPrefix(name, "New") ||
		strings.HasPrefix(name, "MustNew") ||
		strings.HasPrefix(name, "Default")
}

// bodyReturnsComposite reports whether fn contains a return statement whose
// value is a composite literal — either &T{...} or T{...}.
func bodyReturnsComposite(fn *ast.FuncDecl) bool {
	if fn.Body == nil {
		return false
	}
	for _, stmt := range fn.Body.List {
		ret, ok := stmt.(*ast.ReturnStmt)
		if !ok {
			continue
		}
		for _, result := range ret.Results {
			if containsCompositeLiteral(result) {
				return true
			}
		}
	}
	return false
}

// containsCompositeLiteral reports whether expr is or directly wraps a
// composite literal: &T{...} or T{...}.
func containsCompositeLiteral(expr ast.Expr) bool {
	switch e := expr.(type) {
	case *ast.CompositeLit:
		return true
	case *ast.UnaryExpr:
		if e.Op == token.AND {
			_, ok := e.X.(*ast.CompositeLit)
			return ok
		}
	}
	return false
}
