package protocol

import (
	"go/ast"
	"go/types"
)

const encodingJSONImportPath = "encoding/json"

func discoverJSONEncodeInterfaceParams(info *types.Info, fn *ast.FuncDecl) map[string]bool {
	if info == nil || fn == nil || fn.Body == nil {
		return nil
	}
	interfaceParams := emptyInterfaceParamNames(info, fn)
	if len(interfaceParams) == 0 {
		return nil
	}

	out := make(map[string]bool)
	ast.Inspect(fn.Body, func(n ast.Node) bool {
		call, ok := n.(*ast.CallExpr)
		if !ok || len(call.Args) == 0 {
			return true
		}
		paramName, ok := call.Args[0].(*ast.Ident)
		if !ok || !interfaceParams[paramName.Name] {
			return true
		}
		if isJSONMarshalCall(info, call) || isJSONEncoderEncodeCall(info, call) {
			out[paramName.Name] = true
		}
		return true
	})
	if len(out) == 0 {
		return nil
	}
	return out
}

func emptyInterfaceParamNames(info *types.Info, fn *ast.FuncDecl) map[string]bool {
	if fn.Type.Params == nil {
		return nil
	}
	names := make(map[string]bool)
	for _, field := range fn.Type.Params.List {
		if !isEmptyInterfaceExpr(info, field.Type) {
			continue
		}
		for _, name := range field.Names {
			if name.Name != "" && name.Name != "_" {
				names[name.Name] = true
			}
		}
	}
	if len(names) == 0 {
		return nil
	}
	return names
}

func isEmptyInterfaceExpr(info *types.Info, expr ast.Expr) bool {
	switch e := expr.(type) {
	case *ast.InterfaceType:
		return e.Methods == nil || len(e.Methods.List) == 0
	case *ast.Ident:
		if e.Name == "any" {
			return true
		}
	}
	if info == nil || expr == nil {
		return false
	}
	tv, ok := info.Types[expr]
	if !ok || tv.Type == nil {
		return false
	}
	iface, ok := tv.Type.Underlying().(*types.Interface)
	return ok && iface.NumMethods() == 0
}

func isTypeInfoEmptyInterface(t TypeInfo) bool {
	return t.Kind == "opaque" && (t.Label == "interface" || t.Label == "interface{}" || t.Label == "any")
}

func isJSONMarshalCall(info *types.Info, call *ast.CallExpr) bool {
	sel, ok := call.Fun.(*ast.SelectorExpr)
	return ok && sel.Sel.Name == "Marshal" && selectorPackagePath(info, sel) == encodingJSONImportPath
}

func isJSONEncoderEncodeCall(info *types.Info, call *ast.CallExpr) bool {
	encodeSel, ok := call.Fun.(*ast.SelectorExpr)
	if !ok || encodeSel.Sel.Name != "Encode" {
		return false
	}
	newEncoderCall, ok := encodeSel.X.(*ast.CallExpr)
	if !ok {
		return false
	}
	newEncoderSel, ok := newEncoderCall.Fun.(*ast.SelectorExpr)
	return ok && newEncoderSel.Sel.Name == "NewEncoder" && selectorPackagePath(info, newEncoderSel) == encodingJSONImportPath
}

func selectorPackagePath(info *types.Info, sel *ast.SelectorExpr) string {
	if info == nil || sel == nil {
		return ""
	}
	if obj := info.Uses[sel.Sel]; obj != nil && obj.Pkg() != nil {
		return obj.Pkg().Path()
	}
	ident, ok := sel.X.(*ast.Ident)
	if !ok {
		return ""
	}
	obj := info.Uses[ident]
	if obj == nil {
		obj = info.Defs[ident]
	}
	pkgName, ok := obj.(*types.PkgName)
	if !ok || pkgName.Imported() == nil {
		return ""
	}
	return pkgName.Imported().Path()
}
