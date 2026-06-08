package protocol

import (
	"go/ast"
	"go/token"
	"go/types"

	"golang.org/x/tools/go/packages"
)

// ReceiverRequiresConstruction reports whether the receiver type of `target`
// has a zero value that is unlikely to exercise meaningful behavior — i.e.
// the underlying struct carries unexported reference-typed fields that a
// constructor is expected to initialize.
//
// The check is conservative: it returns true only when at least one
// unexported field is a map, channel, function, interface, or pointer type.
// Slices and arrays are intentionally excluded — zero-value slices range
// safely as empty sequences and rarely produce false-meaningful behavior.
// Numeric / string / bool fields are excluded — their zero value is well
// defined.
//
// Returns false for nil package, nil target, free-function targets,
// interface receivers (already short-circuited upstream), generic-unbound
// receivers, named primitives, or any case where the receiver type cannot
// be resolved to a struct shape.
//
// Callers wire the result through PlanOptions.ReceiverRequiresConstruction
// (and through the synthesizeExecuteReceiverKind path in handler.go) so
// that PlanReceivers emits an UnsatisfiedRequirementKindRequiresConstruction
// instead of a fallback zero-value plan when no real strategy applies
// (str-g7h7).
func ReceiverRequiresConstruction(pkg *packages.Package, target *DiscoveredTarget) bool {
	if pkg == nil || pkg.TypesInfo == nil || target == nil || target.Receiver == nil {
		return false
	}
	if target.Receiver.IsInterface {
		return false
	}
	scope := pkg.Types.Scope()
	if scope == nil {
		return false
	}
	obj := scope.Lookup(target.Receiver.TypeName)
	if obj == nil {
		return false
	}
	named, ok := obj.Type().(*types.Named)
	if !ok {
		return false
	}
	st, ok := named.Underlying().(*types.Struct)
	if !ok {
		return false
	}
	dangerousFields := make(map[string]receiverFieldKind)
	for i := 0; i < st.NumFields(); i++ {
		f := st.Field(i)
		if f.Exported() {
			// Exported fields can be initialized by callers via composite
			// literals; the receiver planner already emits the
			// composite-literal strategy for that case. We only flag types
			// whose required state is hidden behind unexported fields.
			continue
		}
		if kind, ok := fieldRequiresInitialization(f.Type()); ok {
			dangerousFields[f.Name()] = kind
		}
	}
	if len(dangerousFields) == 0 {
		return false
	}
	fn := findFuncDeclByBareName(pkg, target.QualifiedName)
	if fn == nil {
		fn = findFuncDeclByBareName(pkg, target.SymbolName)
	}
	if fn == nil || fn.Body == nil {
		return true
	}
	recvName := receiverName(fn)
	if recvName == "" {
		return false
	}
	return methodUsesDangerousReceiverField(fn.Body, recvName, dangerousFields)
}

type receiverFieldKind string

const (
	receiverFieldMap       receiverFieldKind = "map"
	receiverFieldChan      receiverFieldKind = "chan"
	receiverFieldFunc      receiverFieldKind = "func"
	receiverFieldInterface receiverFieldKind = "interface"
	receiverFieldPointer   receiverFieldKind = "pointer"
)

// fieldRequiresInitialization reports whether a field type is a reference
// type whose zero value (nil) is dangerous to use without prior
// initialization. The set mirrors the canonical "nil panics" inventory:
// map reads/writes never panic on nil but writes do; channel sends/receives
// block forever; function calls panic; interface method calls panic;
// pointer dereferences panic.
func fieldRequiresInitialization(t types.Type) (receiverFieldKind, bool) {
	switch t.Underlying().(type) {
	case *types.Map:
		return receiverFieldMap, true
	case *types.Chan:
		return receiverFieldChan, true
	case *types.Signature:
		return receiverFieldFunc, true
	case *types.Interface:
		return receiverFieldInterface, true
	case *types.Pointer:
		return receiverFieldPointer, true
	}
	return "", false
}

func receiverName(fn *ast.FuncDecl) string {
	if fn == nil || fn.Recv == nil || len(fn.Recv.List) == 0 {
		return ""
	}
	if len(fn.Recv.List[0].Names) == 0 {
		return ""
	}
	return fn.Recv.List[0].Names[0].Name
}

func methodUsesDangerousReceiverField(
	body *ast.BlockStmt,
	recvName string,
	fields map[string]receiverFieldKind,
) bool {
	unsafeUse := false
	guardedFields := receiverFieldsNilChecked(body, recvName, fields)
	ast.Inspect(body, func(n ast.Node) bool {
		if unsafeUse || n == nil {
			return false
		}
		switch node := n.(type) {
		case *ast.AssignStmt:
			for _, lhs := range node.Lhs {
				if writesDangerousReceiverField(lhs, recvName, fields) {
					unsafeUse = true
					return false
				}
			}
		case *ast.IncDecStmt:
			if writesDangerousReceiverField(node.X, recvName, fields) {
				unsafeUse = true
				return false
			}
		case *ast.SendStmt:
			if containsDangerousReceiverField(node.Chan, recvName, fields) {
				unsafeUse = true
				return false
			}
		case *ast.RangeStmt:
			if fieldName, ok := directReceiverField(node.X, recvName); ok && fields[fieldName] == receiverFieldChan {
				unsafeUse = true
				return false
			}
		case *ast.UnaryExpr:
			if (node.Op == token.ARROW || node.Op == token.MUL) &&
				containsDangerousReceiverField(node.X, recvName, fields) {
				unsafeUse = true
				return false
			}
		case *ast.CallExpr:
			if callUsesDangerousReceiverField(node, recvName, fields, guardedFields) {
				unsafeUse = true
				return false
			}
		case *ast.SelectorExpr:
			if selectorDereferencesDangerousReceiverField(node, recvName, fields, guardedFields) {
				unsafeUse = true
				return false
			}
		}
		return true
	})
	return unsafeUse
}

func receiverFieldsNilChecked(
	body *ast.BlockStmt,
	recvName string,
	fields map[string]receiverFieldKind,
) map[string]bool {
	guarded := map[string]bool{}
	ast.Inspect(body, func(n ast.Node) bool {
		bin, ok := n.(*ast.BinaryExpr)
		if !ok || (bin.Op != token.NEQ && bin.Op != token.EQL) {
			return true
		}
		if fieldName, ok := nilCheckedReceiverField(bin.X, bin.Y, recvName, fields); ok {
			guarded[fieldName] = true
		}
		if fieldName, ok := nilCheckedReceiverField(bin.Y, bin.X, recvName, fields); ok {
			guarded[fieldName] = true
		}
		return true
	})
	return guarded
}

func nilCheckedReceiverField(
	fieldExpr ast.Expr,
	nilExpr ast.Expr,
	recvName string,
	fields map[string]receiverFieldKind,
) (string, bool) {
	ident, ok := unwrapParen(nilExpr).(*ast.Ident)
	if !ok || ident.Name != "nil" {
		return "", false
	}
	fieldName, ok := directReceiverField(fieldExpr, recvName)
	if !ok {
		return "", false
	}
	switch fields[fieldName] {
	case receiverFieldPointer, receiverFieldInterface:
		return fieldName, true
	default:
		return "", false
	}
}

func writesDangerousReceiverField(expr ast.Expr, recvName string, fields map[string]receiverFieldKind) bool {
	switch e := unwrapParen(expr).(type) {
	case *ast.IndexExpr:
		if fieldName, ok := directReceiverField(e.X, recvName); ok && fields[fieldName] == receiverFieldMap {
			return true
		}
	case *ast.SelectorExpr:
		if fieldName, ok := directReceiverField(e.X, recvName); ok {
			return fields[fieldName] == receiverFieldPointer || fields[fieldName] == receiverFieldInterface
		}
	}
	return false
}

func callUsesDangerousReceiverField(
	call *ast.CallExpr,
	recvName string,
	fields map[string]receiverFieldKind,
	guardedFields map[string]bool,
) bool {
	if sel, ok := unwrapParen(call.Fun).(*ast.SelectorExpr); ok {
		if containsDangerousReceiverField(sel.X, recvName, fields, guardedFields) {
			return true
		}
	}
	if ident, ok := unwrapParen(call.Fun).(*ast.Ident); ok {
		switch ident.Name {
		case "len", "cap", "delete", "clear":
			return false
		}
	}
	for _, arg := range call.Args {
		if fieldName, ok := directReceiverField(arg, recvName); ok && fields[fieldName] == receiverFieldFunc {
			return true
		}
	}
	return false
}

func selectorDereferencesDangerousReceiverField(
	sel *ast.SelectorExpr,
	recvName string,
	fields map[string]receiverFieldKind,
	guardedFields map[string]bool,
) bool {
	fieldName, ok := directReceiverField(sel.X, recvName)
	if !ok {
		return false
	}
	switch fields[fieldName] {
	case receiverFieldPointer, receiverFieldInterface:
		if guardedFields[fieldName] {
			return false
		}
		return true
	default:
		return false
	}
}

func containsDangerousReceiverField(
	expr ast.Expr,
	recvName string,
	fields map[string]receiverFieldKind,
	guardedFields ...map[string]bool,
) bool {
	found := false
	ast.Inspect(expr, func(n ast.Node) bool {
		if found || n == nil {
			return false
		}
		e, ok := n.(ast.Expr)
		if !ok {
			return true
		}
		fieldName, ok := directReceiverField(e, recvName)
		if ok {
			if kind, fieldFound := fields[fieldName]; fieldFound {
				if len(guardedFields) > 0 &&
					guardedFields[0][fieldName] &&
					(kind == receiverFieldPointer || kind == receiverFieldInterface) {
					return false
				}
				found = true
			}
			return false
		}
		return true
	})
	return found
}

func directReceiverField(expr ast.Expr, recvName string) (string, bool) {
	sel, ok := unwrapParen(expr).(*ast.SelectorExpr)
	if !ok {
		return "", false
	}
	ident, ok := unwrapParen(sel.X).(*ast.Ident)
	if !ok || ident.Name != recvName {
		return "", false
	}
	return sel.Sel.Name, true
}

func unwrapParen(expr ast.Expr) ast.Expr {
	for {
		paren, ok := expr.(*ast.ParenExpr)
		if !ok {
			return expr
		}
		expr = paren.X
	}
}
