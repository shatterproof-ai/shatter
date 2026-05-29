package protocol

import (
	"go/ast"
	"go/types"
)

// HTTPHandlerAdapterID is the adapter ID for net/http handler functions.
const HTTPHandlerAdapterID = "go/http-handler"

// recognizeHTTPHandler checks whether fn has the signature
// func(http.ResponseWriter, *http.Request) and returns an InvocationModel
// with synthetic HTTP request params if so. Returns nil for non-handlers.
func recognizeHTTPHandler(fn *ast.FuncDecl, info *types.Info) *InvocationModel {
	if fn.Recv != nil {
		return nil
	}
	if fn.Type.Params == nil {
		return nil
	}

	// Collect the parameter types, flattening grouped params (e.g., "a, b int").
	var paramTypes []types.Type
	for _, field := range fn.Type.Params.List {
		t := info.TypeOf(field.Type)
		if t == nil {
			return nil
		}
		count := len(field.Names)
		if count == 0 {
			count = 1 // unnamed parameter
		}
		for range count {
			paramTypes = append(paramTypes, t)
		}
	}

	// A net/http handler has exactly 2 params: (http.ResponseWriter, *http.Request).
	// Method receivers are separate from fn.Type.Params, so they don't interfere.
	if len(paramTypes) != 2 {
		return nil
	}
	if !isHTTPResponseWriter(paramTypes[0]) || !isHTTPRequest(paramTypes[1]) {
		return nil
	}

	return &InvocationModel{
		Kind:      "adapter",
		AdapterID: HTTPHandlerAdapterID,
		SyntheticParams: []ParamInfo{
			{Name: "method", Type: TypeInfo{Kind: "str", Label: "string"}},
			{Name: "path", Type: TypeInfo{Kind: "str", Label: "string"}},
			{Name: "headers", Type: TypeInfo{Kind: "object", Fields: []ObjectField{}}},
			{Name: "body", Type: TypeInfo{Kind: "str", Label: "string"}},
		},
	}
}

// isHTTPResponseWriter returns true if t is the net/http.ResponseWriter interface.
func isHTTPResponseWriter(t types.Type) bool {
	named, ok := t.(*types.Named)
	if !ok {
		return false
	}
	obj := named.Obj()
	return obj != nil && obj.Pkg() != nil &&
		obj.Pkg().Path() == "net/http" && obj.Name() == "ResponseWriter"
}

// isHTTPRequest returns true if t is *net/http.Request.
func isHTTPRequest(t types.Type) bool {
	ptr, ok := t.(*types.Pointer)
	if !ok {
		return false
	}
	named, ok := ptr.Elem().(*types.Named)
	if !ok {
		return false
	}
	obj := named.Obj()
	return obj != nil && obj.Pkg() != nil &&
		obj.Pkg().Path() == "net/http" && obj.Name() == "Request"
}
