package protocol

import (
	"fmt"
	"go/ast"
	"go/token"
	"go/types"
	"path"
	"strings"
)

// GinAdapterID is the adapter ID for Gin handler functions.
const GinAdapterID = "go/gin"

// Gin characteristic API methods — calls on *gin.Context that strongly
// indicate a Gin handler even without direct parameter type matching.
var ginCharacteristicMethods = map[string]bool{
	"JSON":            true,
	"XML":             true,
	"YAML":            true,
	"String":          true,
	"HTML":            true,
	"Data":            true,
	"Redirect":        true,
	"Param":           true,
	"Query":           true,
	"PostForm":        true,
	"Bind":            true,
	"BindJSON":        true,
	"BindXML":         true,
	"ShouldBind":      true,
	"ShouldBindJSON":  true,
	"AbortWithStatus": true,
	"AbortWithError":  true,
	"Status":          true,
	"Header":          true,
	"SetCookie":       true,
	"GetHeader":       true,
}

// RecognizeNetHTTPHandlers detects net/http handler functions and emits adapter hints.
// Complements recognizeHTTPHandler (in nethttp_recognizer.go) which sets InvocationModel
// for exact (ResponseWriter, *Request) matches. This function also detects partial matches
// (e.g., only ResponseWriter) and ServeHTTP methods, emitting AdapterHints with reasons.
// Returns a parallel slice — one *AdapterHint per input function (nil if not a handler).
func RecognizeNetHTTPHandlers(fset *token.FileSet, file *ast.File, info *types.Info, functions []FunctionAnalysis) []*AdapterHint {
	hints := make([]*AdapterHint, len(functions))

	imports := buildImportAliasMap(file)
	if _, ok := imports["net/http"]; !ok {
		return hints
	}

	for i, fa := range functions {
		fn := findFuncDeclByLine(fset, file, fa.Name, fa.StartLine)
		if fn == nil {
			continue
		}
		hints[i] = recognizeNetHTTPHint(fn, info, imports)
	}
	return hints
}

// RecognizeGinHandlers detects Gin handler functions.
// Returns a parallel slice — one *AdapterHint per input function (nil if not a handler).
func RecognizeGinHandlers(fset *token.FileSet, file *ast.File, info *types.Info, functions []FunctionAnalysis) []*AdapterHint {
	hints := make([]*AdapterHint, len(functions))

	imports := buildImportAliasMap(file)
	ginImportPath := ""
	for importPath := range imports {
		if importPath == "github.com/gin-gonic/gin" {
			ginImportPath = importPath
			break
		}
	}
	if ginImportPath == "" {
		return hints
	}

	for i, fa := range functions {
		fn := findFuncDeclByLine(fset, file, fa.Name, fa.StartLine)
		if fn == nil {
			continue
		}
		hints[i] = recognizeGinHint(fn, info, imports, ginImportPath)
	}
	return hints
}

func recognizeNetHTTPHint(fn *ast.FuncDecl, info *types.Info, imports map[string]string) *AdapterHint {
	var reasons []string

	// Check for ServeHTTP method signature.
	if fn.Recv != nil && fn.Name.Name == "ServeHTTP" {
		reasons = append(reasons, "Method ServeHTTP implements net/http.Handler interface")
	}

	// Check parameters for http.ResponseWriter and *http.Request.
	hasResponseWriter := false
	hasRequest := false
	if fn.Type.Params != nil {
		for _, field := range fn.Type.Params.List {
			if paramMatchesPkgType(field, info, imports, "net/http", "ResponseWriter", false) {
				for _, name := range field.Names {
					reasons = append(reasons, fmt.Sprintf("Parameter %s has type net/http.ResponseWriter", name.Name))
				}
				if len(field.Names) == 0 {
					reasons = append(reasons, "Parameter has type net/http.ResponseWriter")
				}
				hasResponseWriter = true
			}
			if paramMatchesPkgType(field, info, imports, "net/http", "Request", true) {
				for _, name := range field.Names {
					reasons = append(reasons, fmt.Sprintf("Parameter %s has type *net/http.Request", name.Name))
				}
				if len(field.Names) == 0 {
					reasons = append(reasons, "Parameter has type *net/http.Request")
				}
				hasRequest = true
			}
		}
	}

	if len(reasons) == 0 {
		return nil
	}

	confidence := "medium"
	if (hasResponseWriter && hasRequest) || (fn.Recv != nil && fn.Name.Name == "ServeHTTP") {
		confidence = "high"
	}

	return &AdapterHint{
		Adapter:    ExecutionAdapter{ID: HTTPHandlerAdapterID},
		Confidence: confidence,
		Reasons:    reasons,
	}
}

func recognizeGinHint(fn *ast.FuncDecl, info *types.Info, imports map[string]string, ginImportPath string) *AdapterHint {
	var reasons []string
	var ginParamName string

	// Check parameters for *gin.Context.
	if fn.Type.Params != nil {
		for _, field := range fn.Type.Params.List {
			if paramMatchesPkgType(field, info, imports, ginImportPath, "Context", true) {
				for _, name := range field.Names {
					reasons = append(reasons, fmt.Sprintf("Parameter %s has type *gin.Context (import: %s)", name.Name, ginImportPath))
					ginParamName = name.Name
				}
				if len(field.Names) == 0 {
					reasons = append(reasons, fmt.Sprintf("Parameter has type *gin.Context (import: %s)", ginImportPath))
				}
			}
		}
	}

	// Walk body for characteristic API calls on the gin.Context parameter.
	if ginParamName != "" && fn.Body != nil {
		apiReasons := collectGinAPICalls(fn.Body, ginParamName)
		reasons = append(reasons, apiReasons...)
	}

	if len(reasons) == 0 {
		return nil
	}

	confidence := "high"
	if ginParamName == "" {
		confidence = "medium"
	}

	return &AdapterHint{
		Adapter:    ExecutionAdapter{ID: GinAdapterID},
		Confidence: confidence,
		Reasons:    reasons,
	}
}

// collectGinAPICalls walks an AST block for characteristic gin.Context method calls.
func collectGinAPICalls(body *ast.BlockStmt, receiverName string) []string {
	seen := make(map[string]bool)
	var reasons []string

	ast.Inspect(body, func(n ast.Node) bool {
		call, ok := n.(*ast.CallExpr)
		if !ok {
			return true
		}
		sel, ok := call.Fun.(*ast.SelectorExpr)
		if !ok {
			return true
		}
		ident, ok := sel.X.(*ast.Ident)
		if !ok || ident.Name != receiverName {
			return true
		}
		method := sel.Sel.Name
		if ginCharacteristicMethods[method] && !seen[method] {
			seen[method] = true
			reasons = append(reasons, fmt.Sprintf("Calls %s.%s (characteristic Gin API)", receiverName, method))
		}
		return true
	})
	return reasons
}

// buildImportAliasMap maps import paths to their local aliases.
// For named imports the alias is the declared name; for unnamed imports
// it is the last path component; dot imports map to ".".
func buildImportAliasMap(file *ast.File) map[string]string {
	m := make(map[string]string, len(file.Imports))
	for _, imp := range file.Imports {
		importPath := strings.Trim(imp.Path.Value, `"`)
		if imp.Name != nil {
			m[importPath] = imp.Name.Name
		} else {
			m[importPath] = path.Base(importPath)
		}
	}
	return m
}

// findFuncDeclByLine locates the ast.FuncDecl matching a FunctionAnalysis by name and start line.
func findFuncDeclByLine(fset *token.FileSet, file *ast.File, name string, startLine int) *ast.FuncDecl {
	for _, decl := range file.Decls {
		fn, ok := decl.(*ast.FuncDecl)
		if !ok || fn.Body == nil {
			continue
		}
		if fn.Name.Name == name && fset.Position(fn.Pos()).Line == startLine {
			return fn
		}
	}
	return nil
}

// paramMatchesPkgType checks if a field's type matches pkgPath.typeName,
// optionally wrapped in a pointer. Tries the type checker first, then
// falls back to AST matching for unresolved imports.
func paramMatchesPkgType(field *ast.Field, info *types.Info, imports map[string]string, pkgPath, typeName string, wantPointer bool) bool {
	// Strategy 1: type checker (works for stdlib and resolved imports).
	if info != nil {
		typeExpr := field.Type
		if wantPointer {
			if star, ok := typeExpr.(*ast.StarExpr); ok {
				typeExpr = star.X
			}
		}
		if tv, ok := info.Types[typeExpr]; ok {
			if named, ok := tv.Type.(*types.Named); ok {
				obj := named.Obj()
				if obj.Pkg() != nil && obj.Pkg().Path() == pkgPath && obj.Name() == typeName {
					return true
				}
			}
		}
		// Also check the full field type (including pointer) for named types.
		if tv, ok := info.Types[field.Type]; ok {
			underlying := tv.Type
			if wantPointer {
				if ptr, ok := underlying.(*types.Pointer); ok {
					underlying = ptr.Elem()
				}
			}
			if named, ok := underlying.(*types.Named); ok {
				obj := named.Obj()
				if obj.Pkg() != nil && obj.Pkg().Path() == pkgPath && obj.Name() == typeName {
					return true
				}
			}
		}
	}

	// Strategy 2: AST fallback (for unresolved third-party imports).
	alias, hasImport := imports[pkgPath]
	if !hasImport {
		return false
	}
	return astTypeMatches(field.Type, alias, typeName, wantPointer)
}

// astTypeMatches checks if an AST type expression matches alias.typeName,
// optionally wrapped in a pointer (*ast.StarExpr).
func astTypeMatches(expr ast.Expr, alias, typeName string, wantPointer bool) bool {
	if wantPointer {
		star, ok := expr.(*ast.StarExpr)
		if !ok {
			return false
		}
		expr = star.X
	}
	sel, ok := expr.(*ast.SelectorExpr)
	if !ok {
		return false
	}
	ident, ok := sel.X.(*ast.Ident)
	if !ok {
		return false
	}
	return ident.Name == alias && sel.Sel.Name == typeName
}
