package protocol

import (
	"go/ast"
	"go/token"
	"go/types"
)

// targetKindOf returns the TargetKind for a function declaration.
func targetKindOf(fn *ast.FuncDecl) TargetKind {
	if fn.Recv != nil && len(fn.Recv.List) > 0 {
		return TargetKindMethod
	}
	return TargetKindFunction
}

// receiverShapeOf extracts the ReceiverShape from a method declaration.
// Returns nil for free functions.
func receiverShapeOf(fn *ast.FuncDecl) *ReceiverShape {
	if fn.Recv == nil || len(fn.Recv.List) == 0 {
		return nil
	}
	expr := fn.Recv.List[0].Type
	if star, ok := expr.(*ast.StarExpr); ok {
		if ident, ok := star.X.(*ast.Ident); ok {
			return &ReceiverShape{TypeName: ident.Name, IsPointer: true}
		}
	}
	if ident, ok := expr.(*ast.Ident); ok {
		return &ReceiverShape{TypeName: ident.Name, IsPointer: false}
	}
	return nil
}

// qualifiedNameOf returns the qualified name of a function declaration.
// Free functions return their bare name; methods return (T).Name or (*T).Name.
func qualifiedNameOf(fn *ast.FuncDecl) string {
	recv := receiverShapeOf(fn)
	if recv == nil {
		return fn.Name.Name
	}
	if recv.IsPointer {
		return "(*" + recv.TypeName + ")." + fn.Name.Name
	}
	return "(" + recv.TypeName + ")." + fn.Name.Name
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
		Receiver:      receiverShapeOf(fn),
		Parameters:    extractParamsWithContext(fn, info, nil),
		Results:       extractResultTypes(fn, info),
		Visibility:    visibility,
	}
}
