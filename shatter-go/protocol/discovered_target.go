package protocol

import (
	"go/ast"
	"go/token"
	"go/types"
	"strings"
)

// targetKindOf returns the TargetKind for a function declaration.
func targetKindOf(fn *ast.FuncDecl) TargetKind {
	if fn.Recv != nil && len(fn.Recv.List) > 0 {
		return TargetKindMethod
	}
	return TargetKindFunction
}

// receiverShapeOf extracts the ReceiverShape from a method declaration.
// Returns nil for free functions. info may be nil; IsInterface is only set when info is non-nil.
func receiverShapeOf(fn *ast.FuncDecl, info *types.Info) *ReceiverShape {
	if fn.Recv == nil || len(fn.Recv.List) == 0 {
		return nil
	}
	expr := fn.Recv.List[0].Type
	var shape *ReceiverShape
	if star, ok := expr.(*ast.StarExpr); ok {
		if ident, ok := star.X.(*ast.Ident); ok {
			shape = &ReceiverShape{TypeName: ident.Name, IsPointer: true}
		}
	}
	if shape == nil {
		if ident, ok := expr.(*ast.Ident); ok {
			shape = &ReceiverShape{TypeName: ident.Name, IsPointer: false}
		}
	}
	if shape != nil && info != nil {
		shape.IsInterface = receiverIsInterface(fn, info)
	}
	return shape
}

// qualifiedNameOf returns the qualified name of a function declaration.
// Free functions return their bare name; methods return (T).Name or (*T).Name.
func qualifiedNameOf(fn *ast.FuncDecl) string {
	recv := receiverShapeOf(fn, nil)
	if recv == nil {
		return fn.Name.Name
	}
	if recv.IsPointer {
		return "(*" + recv.TypeName + ")." + fn.Name.Name
	}
	return "(" + recv.TypeName + ")." + fn.Name.Name
}

// fnHasTypeParams reports whether fn declares generic type parameters.
func fnHasTypeParams(fn *ast.FuncDecl) bool {
	return fn.Type.TypeParams != nil && len(fn.Type.TypeParams.List) > 0
}

func typeParamsOf(fset *token.FileSet, fn *ast.FuncDecl) []TypeParamInfo {
	if fn.Type.TypeParams == nil || len(fn.Type.TypeParams.List) == 0 {
		return nil
	}
	var params []TypeParamInfo
	for _, field := range fn.Type.TypeParams.List {
		constraint := "any"
		if field.Type != nil {
			constraint = strings.TrimSpace(exprText(fset, field.Type))
			if constraint == "" {
				constraint = "any"
			}
		}
		for _, name := range field.Names {
			params = append(params, TypeParamInfo{
				Name:       name.Name,
				Constraint: constraint,
			})
		}
	}
	return params
}

// receiverIsInterface reports whether the receiver type's underlying type is an interface.
func receiverIsInterface(fn *ast.FuncDecl, info *types.Info) bool {
	if fn.Recv == nil || len(fn.Recv.List) == 0 {
		return false
	}
	expr := fn.Recv.List[0].Type
	if star, ok := expr.(*ast.StarExpr); ok {
		expr = star.X
	}
	t := info.TypeOf(expr)
	if t == nil {
		return false
	}
	_, isIface := t.Underlying().(*types.Interface)
	return isIface
}

// fnHasCGoDep reports whether any parameter or result type of fn uses the CGo "C" pseudo-package.
func fnHasCGoDep(fn *ast.FuncDecl, info *types.Info) bool {
	check := func(expr ast.Expr) bool {
		return cgoDependentType(info.TypeOf(expr))
	}
	if fn.Type.Params != nil {
		for _, field := range fn.Type.Params.List {
			if check(field.Type) {
				return true
			}
		}
	}
	if fn.Type.Results != nil {
		for _, field := range fn.Type.Results.List {
			if check(field.Type) {
				return true
			}
		}
	}
	return false
}

func cgoDependentType(t types.Type) bool {
	if t == nil {
		return false
	}
	if named, ok := t.(*types.Named); ok {
		pkg := named.Obj().Pkg()
		return pkg != nil && pkg.Path() == "C"
	}
	if ptr, ok := t.(*types.Pointer); ok {
		return cgoDependentType(ptr.Elem())
	}
	return false
}

// extractResultTypes returns a TypeInfo for each result of fn, in declaration order.
// Named and unnamed results are both included; named multi-return fields expand to
// one entry per name.
func extractResultTypes(fn *ast.FuncDecl, info *types.Info) []TypeInfo {
	if fn.Type.Results == nil {
		return []TypeInfo{}
	}
	var results []TypeInfo
	for _, field := range fn.Type.Results.List {
		ti := goTypeFromExpr(field.Type, info)
		count := len(field.Names)
		if count == 0 {
			count = 1
		}
		for i := 0; i < count; i++ {
			results = append(results, ti)
		}
	}
	if results == nil {
		return []TypeInfo{}
	}
	return results
}

// BuildDiscoveredTarget constructs a DiscoveredTarget from a parsed function
// declaration and its type-checked context. packagePath, packageName, and
// filePath are taken from the enclosing package metadata (e.g. pkg.PkgPath,
// pkg.Name, and the position from fset).
//
// The returned ID is stable across repeated analysis runs: it is derived
// solely from packagePath and the qualified symbol name.
func BuildDiscoveredTarget(
	fset *token.FileSet,
	fn *ast.FuncDecl,
	info *types.Info,
	packagePath string,
	packageName string,
	filePath string,
) DiscoveredTarget {
	qualName := qualifiedNameOf(fn)

	visibility := "unexported"
	if ast.IsExported(fn.Name.Name) {
		visibility = "exported"
	}

	startPos := fset.Position(fn.Pos())
	endPos := fset.Position(fn.End())

	return DiscoveredTarget{
		ID:            packagePath + ":" + qualName,
		PackagePath:   packagePath,
		PackageName:   packageName,
		FilePath:      filePath,
		StartLine:     startPos.Line,
		EndLine:       endPos.Line,
		SymbolName:    fn.Name.Name,
		QualifiedName: qualName,
		Kind:          targetKindOf(fn),
		Receiver:      receiverShapeOf(fn, info),
		Parameters:    extractParamsWithContext(fn, info, nil),
		Results:       extractResultTypes(fn, info),
		Visibility:    visibility,
		TypeParams:    typeParamsOf(fset, fn),
		HasTypeParams: fnHasTypeParams(fn),
		HasCGoDep:     fnHasCGoDep(fn, info),
		IsTestFile:    strings.HasSuffix(filePath, "_test.go"),
	}
}
