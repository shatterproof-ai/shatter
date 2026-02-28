package protocol

import (
	"fmt"
	"go/ast"
	"go/importer"
	"go/parser"
	"go/printer"
	"go/token"
	"go/types"
	"strconv"
	"strings"
)

// AnalyzeFile parses a Go source file and returns analysis for all exported
// functions, or a single function if functionName is non-empty.
func AnalyzeFile(filePath string, functionName string) ([]FunctionAnalysis, error) {
	fset := token.NewFileSet()
	file, err := parser.ParseFile(fset, filePath, nil, parser.ParseComments)
	if err != nil {
		return nil, fmt.Errorf("parse error: %w", err)
	}

	info := typeCheck(fset, file)

	var results []FunctionAnalysis
	for _, decl := range file.Decls {
		fn, ok := decl.(*ast.FuncDecl)
		if !ok || fn.Body == nil {
			continue
		}
		if functionName != "" && fn.Name.Name != functionName {
			continue
		}
		analysis := analyzeFunc(fset, fn, info)
		results = append(results, analysis)
	}

	if functionName != "" && len(results) == 0 {
		return nil, fmt.Errorf("function not found: %s", functionName)
	}
	return results, nil
}

// typeCheck runs the Go type checker and returns the resulting Info.
// Import errors are silently ignored so we can still extract types from
// successfully resolved identifiers.
func typeCheck(fset *token.FileSet, file *ast.File) *types.Info {
	info := &types.Info{
		Types: make(map[ast.Expr]types.TypeAndValue),
		Defs:  make(map[*ast.Ident]types.Object),
		Uses:  make(map[*ast.Ident]types.Object),
	}
	conf := types.Config{
		Importer: importer.Default(),
		Error:    func(error) {}, // swallow errors from missing imports
	}
	conf.Check(file.Name.Name, fset, []*ast.File{file}, info) //nolint:errcheck
	return info
}

func analyzeFunc(fset *token.FileSet, fn *ast.FuncDecl, info *types.Info) FunctionAnalysis {
	params := extractParams(fn, info)
	returnType := extractReturnType(fn, info)
	paramNames := paramNameSet(params)
	branches := extractBranches(fset, fn.Body, paramNames)
	deps := extractDependencies(fset, fn.Body, info)

	startLine := fset.Position(fn.Pos()).Line
	endLine := fset.Position(fn.End()).Line

	return FunctionAnalysis{
		Name:         fn.Name.Name,
		Exported:     ast.IsExported(fn.Name.Name),
		Params:       params,
		Branches:     branches,
		Dependencies: deps,
		ReturnType:   returnType,
		StartLine:    startLine,
		EndLine:      endLine,
	}
}

func paramNameSet(params []ParamInfo) map[string]bool {
	m := make(map[string]bool, len(params))
	for _, p := range params {
		m[p.Name] = true
	}
	return m
}

// --- Parameter and Return Type Extraction ---

func extractParams(fn *ast.FuncDecl, info *types.Info) []ParamInfo {
	if fn.Type.Params == nil {
		return []ParamInfo{}
	}
	var params []ParamInfo
	for _, field := range fn.Type.Params.List {
		ti := goTypeFromExpr(field.Type, info)
		for _, name := range field.Names {
			params = append(params, ParamInfo{
				Name: name.Name,
				Type: ti,
			})
		}
	}
	if params == nil {
		return []ParamInfo{}
	}
	return params
}

func extractReturnType(fn *ast.FuncDecl, info *types.Info) TypeInfo {
	results := fn.Type.Results
	if results == nil || len(results.List) == 0 {
		return TypeInfo{Kind: "unknown"}
	}
	if len(results.List) == 1 && len(results.List[0].Names) <= 1 {
		return goTypeFromExpr(results.List[0].Type, info)
	}
	// Multiple return values → tuple as object
	var fields []ObjectField
	idx := 0
	for _, field := range results.List {
		ti := goTypeFromExpr(field.Type, info)
		if len(field.Names) == 0 {
			fields = append(fields, ObjectField{
				Name: fmt.Sprintf("_%d", idx),
				Type: ti,
			})
			idx++
		} else {
			for _, name := range field.Names {
				fields = append(fields, ObjectField{
					Name: name.Name,
					Type: ti,
				})
				idx++
			}
		}
	}
	return TypeInfo{Kind: "object", Fields: fields}
}

// --- Go Type → TypeInfo ---

func goTypeFromExpr(expr ast.Expr, info *types.Info) TypeInfo {
	if tv, ok := info.Types[expr]; ok {
		return goTypeToTypeInfo(tv.Type)
	}
	// Fallback: infer from AST when type checker didn't resolve
	return typeInfoFromAST(expr)
}

// opaqueGoTypes maps package paths to sets of type names that represent
// runtime resources (sockets, file handles, database connections, etc.).
var opaqueGoTypes = map[string]map[string]bool{
	"net":          {"Conn": true, "Listener": true, "PacketConn": true},
	"os":           {"File": true},
	"io":           {"Reader": true, "Writer": true, "ReadWriter": true, "Closer": true, "ReadCloser": true, "WriteCloser": true},
	"database/sql": {"DB": true, "Tx": true, "Rows": true},
	"net/http":     {"ResponseWriter": true},
}

// isOpaqueGoType checks whether t is a known opaque resource type.
// Returns the label (e.g. "net.Conn") and true if opaque, or ("", false).
func isOpaqueGoType(t types.Type) (string, bool) {
	if ch, ok := t.(*types.Chan); ok {
		return "chan " + ch.Elem().String(), true
	}
	named, ok := t.(*types.Named)
	if !ok {
		return "", false
	}
	obj := named.Obj()
	pkg := obj.Pkg()
	if pkg == nil {
		return "", false
	}
	if names, found := opaqueGoTypes[pkg.Path()]; found {
		if names[obj.Name()] {
			return pkg.Name() + "." + obj.Name(), true
		}
	}
	return "", false
}

func goTypeToTypeInfo(t types.Type) TypeInfo {
	// Check for opaque resource types (channels, sockets, file handles, etc.)
	if label, ok := isOpaqueGoType(t); ok {
		return TypeInfo{Kind: "opaque", Label: label}
	}
	// Check for pointer-to-opaque (e.g. *os.File)
	if ptr, ok := t.(*types.Pointer); ok {
		if label, ok := isOpaqueGoType(ptr.Elem()); ok {
			return TypeInfo{Kind: "opaque", Label: label}
		}
	}
	// Check for well-known complex types by fully-qualified name
	if named, ok := t.(*types.Named); ok {
		if complexKind := complexKindFromNamed(named); complexKind != "" {
			return TypeInfo{Kind: "complex", ComplexKind: complexKind}
		}
	}
	// Check for pointer-to-named complex types
	if ptr, ok := t.(*types.Pointer); ok {
		if named, ok := ptr.Elem().(*types.Named); ok {
			if complexKind := complexKindFromNamed(named); complexKind != "" {
				return TypeInfo{Kind: "complex", ComplexKind: complexKind}
			}
		}
	}
	switch typ := t.Underlying().(type) {
	case *types.Basic:
		return basicTypeInfo(typ)
	case *types.Slice:
		elem := goTypeToTypeInfo(typ.Elem())
		return TypeInfo{Kind: "array", Element: &elem}
	case *types.Array:
		elem := goTypeToTypeInfo(typ.Elem())
		return TypeInfo{Kind: "array", Element: &elem}
	case *types.Map:
		return mapTypeInfo(typ)
	case *types.Struct:
		return structTypeInfo(typ)
	case *types.Pointer:
		inner := goTypeToTypeInfo(typ.Elem())
		return TypeInfo{Kind: "nullable", Inner: &inner}
	case *types.Chan:
		return TypeInfo{Kind: "opaque", Label: "chan " + typ.Elem().String()}
	case *types.Interface:
		return TypeInfo{Kind: "unknown"}
	case *types.Signature:
		return TypeInfo{Kind: "unknown"}
	case *types.Tuple:
		return TypeInfo{Kind: "unknown"}
	default:
		return TypeInfo{Kind: "unknown"}
	}
}

// complexKindFromNamed maps well-known Go named types to their ComplexKind string.
// Returns "" if the type is not a recognized complex type.
func complexKindFromNamed(named *types.Named) string {
	obj := named.Obj()
	pkg := obj.Pkg()
	name := obj.Name()

	if pkg == nil {
		// Built-in types: error interface
		if name == "error" {
			return "error"
		}
		return ""
	}

	pkgPath := pkg.Path()
	switch {
	case pkgPath == "time" && name == "Time":
		return "date"
	case pkgPath == "time" && name == "Duration":
		return "duration"
	case pkgPath == "net/url" && name == "URL":
		return "url"
	case pkgPath == "regexp" && name == "Regexp":
		return "reg_exp"
	case pkgPath == "net" && name == "IP":
		return "ip_address"
	case pkgPath == "math/big" && name == "Int":
		return "big_int"
	case pkgPath == "math/big" && name == "Rat":
		return "rational"
	case pkgPath == "math/big" && name == "Float":
		return "big_decimal"
	default:
		return ""
	}
}

func basicTypeInfo(b *types.Basic) TypeInfo {
	// Check for rune (int32) and byte (uint8) aliases
	switch b.Kind() {
	case types.Int32:
		// Go's rune is an alias for int32
		return TypeInfo{Kind: "int"}
	case types.Uint8:
		// Go's byte is an alias for uint8
		return TypeInfo{Kind: "int"}
	}
	switch {
	case b.Info()&types.IsInteger != 0:
		return TypeInfo{Kind: "int"}
	case b.Info()&types.IsFloat != 0:
		return TypeInfo{Kind: "float"}
	case b.Info()&types.IsString != 0:
		return TypeInfo{Kind: "str"}
	case b.Info()&types.IsBoolean != 0:
		return TypeInfo{Kind: "bool"}
	default:
		return TypeInfo{Kind: "unknown"}
	}
}

func mapTypeInfo(m *types.Map) TypeInfo {
	keyType := goTypeToTypeInfo(m.Key())
	valType := goTypeToTypeInfo(m.Elem())
	return TypeInfo{
		Kind: "object",
		Fields: []ObjectField{
			{Name: "_key", Type: keyType},
			{Name: "_value", Type: valType},
		},
	}
}

func structTypeInfo(s *types.Struct) TypeInfo {
	fields := make([]ObjectField, s.NumFields())
	for i := 0; i < s.NumFields(); i++ {
		f := s.Field(i)
		fields[i] = ObjectField{
			Name: f.Name(),
			Type: goTypeToTypeInfo(f.Type()),
		}
	}
	return TypeInfo{Kind: "object", Fields: fields}
}

// typeInfoFromAST is a best-effort fallback when the type checker fails.
func typeInfoFromAST(expr ast.Expr) TypeInfo {
	switch e := expr.(type) {
	case *ast.Ident:
		switch e.Name {
		case "int", "int8", "int16", "int32", "int64",
			"uint", "uint8", "uint16", "uint32", "uint64",
			"byte", "rune":
			return TypeInfo{Kind: "int"}
		case "float32", "float64":
			return TypeInfo{Kind: "float"}
		case "string":
			return TypeInfo{Kind: "str"}
		case "bool":
			return TypeInfo{Kind: "bool"}
		default:
			return TypeInfo{Kind: "unknown"}
		}
	case *ast.ArrayType:
		elem := typeInfoFromAST(e.Elt)
		return TypeInfo{Kind: "array", Element: &elem}
	case *ast.StarExpr:
		inner := typeInfoFromAST(e.X)
		return TypeInfo{Kind: "nullable", Inner: &inner}
	case *ast.MapType:
		return TypeInfo{Kind: "unknown"}
	case *ast.InterfaceType:
		return TypeInfo{Kind: "unknown"}
	default:
		return TypeInfo{Kind: "unknown"}
	}
}

// --- Branch Extraction ---

func extractBranches(fset *token.FileSet, body *ast.BlockStmt, params map[string]bool) []BranchInfo {
	var branches []BranchInfo
	nextID := 0

	ast.Inspect(body, func(n ast.Node) bool {
		switch stmt := n.(type) {
		case *ast.IfStmt:
			branches = append(branches, ifBranch(fset, stmt, params, &nextID))
		case *ast.SwitchStmt:
			branches = append(branches, switchBranches(fset, stmt, params, &nextID)...)
		case *ast.ForStmt:
			if stmt.Cond != nil {
				branches = append(branches, forBranch(fset, stmt, params, &nextID))
			}
		case *ast.RangeStmt:
			branches = append(branches, rangeBranch(fset, stmt, params, &nextID))
		case *ast.SelectStmt:
			branches = append(branches, selectBranches(fset, stmt, &nextID)...)
		}
		return true
	})

	if branches == nil {
		return []BranchInfo{}
	}
	return branches
}

func ifBranch(fset *token.FileSet, stmt *ast.IfStmt, params map[string]bool, nextID *int) BranchInfo {
	id := *nextID
	*nextID++
	condText := exprText(fset, stmt.Cond)
	cond := buildSymExpr(stmt.Cond, params)
	branchType := "if"
	return BranchInfo{
		ID:            id,
		Line:          fset.Position(stmt.Pos()).Line,
		ConditionText: condText,
		Condition:     cond,
		BranchType:    branchType,
	}
}

func switchBranches(fset *token.FileSet, stmt *ast.SwitchStmt, params map[string]bool, nextID *int) []BranchInfo {
	var branches []BranchInfo
	for _, clause := range stmt.Body.List {
		cc, ok := clause.(*ast.CaseClause)
		if !ok || cc.List == nil {
			continue // default clause
		}
		for _, expr := range cc.List {
			id := *nextID
			*nextID++
			var condText string
			if stmt.Tag != nil {
				condText = exprText(fset, stmt.Tag) + " == " + exprText(fset, expr)
			} else {
				condText = exprText(fset, expr)
			}
			cond := buildSwitchCaseSymExpr(stmt.Tag, expr, params)
			branches = append(branches, BranchInfo{
				ID:            id,
				Line:          fset.Position(cc.Pos()).Line,
				ConditionText: condText,
				Condition:     cond,
				BranchType:    "switch",
			})
		}
	}
	return branches
}

func forBranch(fset *token.FileSet, stmt *ast.ForStmt, params map[string]bool, nextID *int) BranchInfo {
	id := *nextID
	*nextID++
	return BranchInfo{
		ID:            id,
		Line:          fset.Position(stmt.Pos()).Line,
		ConditionText: exprText(fset, stmt.Cond),
		Condition:     buildSymExpr(stmt.Cond, params),
		BranchType:    "for",
	}
}

func rangeBranch(fset *token.FileSet, stmt *ast.RangeStmt, params map[string]bool, nextID *int) BranchInfo {
	id := *nextID
	*nextID++
	return BranchInfo{
		ID:            id,
		Line:          fset.Position(stmt.Pos()).Line,
		ConditionText: "range " + exprText(fset, stmt.X),
		Condition:     buildSymExpr(stmt.X, params),
		BranchType:    "for",
	}
}

func selectBranches(fset *token.FileSet, stmt *ast.SelectStmt, nextID *int) []BranchInfo {
	var branches []BranchInfo
	for _, clause := range stmt.Body.List {
		cc, ok := clause.(*ast.CommClause)
		if !ok {
			continue
		}
		id := *nextID
		*nextID++
		var condText string
		if cc.Comm != nil {
			condText = stmtText(fset, cc.Comm)
		} else {
			condText = "default"
		}
		branches = append(branches, BranchInfo{
			ID:            id,
			Line:          fset.Position(cc.Pos()).Line,
			ConditionText: condText,
			BranchType:    "select",
		})
	}
	return branches
}

func stmtText(fset *token.FileSet, stmt ast.Stmt) string {
	var buf strings.Builder
	printer.Fprint(&buf, fset, stmt)
	return buf.String()
}

// --- Symbolic Expression Building ---

func buildSymExpr(expr ast.Expr, params map[string]bool) *SymExpr {
	if expr == nil {
		return nil
	}
	switch e := expr.(type) {
	case *ast.BinaryExpr:
		return buildBinOp(e, params)
	case *ast.UnaryExpr:
		return buildUnOp(e, params)
	case *ast.Ident:
		return identSymExpr(e, params)
	case *ast.SelectorExpr:
		return selectorSymExpr(e, params)
	case *ast.BasicLit:
		return litSymExpr(e)
	case *ast.CallExpr:
		return callSymExpr(e, params)
	case *ast.ParenExpr:
		return buildSymExpr(e.X, params)
	default:
		return &SymExpr{Kind: "unknown"}
	}
}

func buildBinOp(expr *ast.BinaryExpr, params map[string]bool) *SymExpr {
	op := tokenToOp(expr.Op)
	left := buildSymExpr(expr.X, params)
	right := buildSymExpr(expr.Y, params)
	return &SymExpr{
		Kind:  "bin_op",
		Op:    op,
		Left:  left,
		Right: right,
	}
}

func buildUnOp(expr *ast.UnaryExpr, params map[string]bool) *SymExpr {
	op := tokenToOp(expr.Op)
	operand := buildSymExpr(expr.X, params)
	return &SymExpr{
		Kind:    "un_op",
		Op:      op,
		Operand: operand,
	}
}

func identSymExpr(ident *ast.Ident, params map[string]bool) *SymExpr {
	if params[ident.Name] {
		return &SymExpr{
			Kind: "param",
			Name: ident.Name,
			Path: []string{},
		}
	}
	// Not a param — treat as unknown variable reference
	return &SymExpr{Kind: "unknown"}
}

func selectorSymExpr(sel *ast.SelectorExpr, params map[string]bool) *SymExpr {
	// Check if the root is a parameter (e.g., order.Priority)
	name, path := flattenSelector(sel)
	if params[name] {
		return &SymExpr{
			Kind: "param",
			Name: name,
			Path: path,
		}
	}
	return &SymExpr{Kind: "unknown"}
}

func flattenSelector(sel *ast.SelectorExpr) (root string, path []string) {
	switch x := sel.X.(type) {
	case *ast.Ident:
		return x.Name, []string{sel.Sel.Name}
	case *ast.SelectorExpr:
		root, innerPath := flattenSelector(x)
		return root, append(innerPath, sel.Sel.Name)
	default:
		return "", nil
	}
}

func litSymExpr(lit *ast.BasicLit) *SymExpr {
	switch lit.Kind {
	case token.INT:
		n, err := strconv.ParseInt(lit.Value, 0, 64)
		if err != nil {
			return &SymExpr{Kind: "unknown"}
		}
		return &SymExpr{Kind: "const", Type: "int", Value: n}
	case token.FLOAT:
		f, err := strconv.ParseFloat(lit.Value, 64)
		if err != nil {
			return &SymExpr{Kind: "unknown"}
		}
		return &SymExpr{Kind: "const", Type: "float", Value: f}
	case token.STRING, token.CHAR:
		// Strip quotes for the value
		val := strings.Trim(lit.Value, "`\"'")
		return &SymExpr{Kind: "const", Type: "str", Value: val}
	default:
		return &SymExpr{Kind: "unknown"}
	}
}

func callSymExpr(call *ast.CallExpr, params map[string]bool) *SymExpr {
	var name string
	switch fn := call.Fun.(type) {
	case *ast.Ident:
		name = fn.Name
	case *ast.SelectorExpr:
		name = exprString(call.Fun)
	default:
		name = "unknown"
	}

	args := make([]SymExpr, len(call.Args))
	for i, arg := range call.Args {
		sym := buildSymExpr(arg, params)
		if sym != nil {
			args[i] = *sym
		} else {
			args[i] = SymExpr{Kind: "unknown"}
		}
	}
	return &SymExpr{
		Kind: "call",
		Name: name,
		Args: args,
	}
}

func buildSwitchCaseSymExpr(tag ast.Expr, caseExpr ast.Expr, params map[string]bool) *SymExpr {
	if tag == nil {
		return buildSymExpr(caseExpr, params)
	}
	left := buildSymExpr(tag, params)
	right := buildSymExpr(caseExpr, params)
	return &SymExpr{
		Kind:  "bin_op",
		Op:    "eq",
		Left:  left,
		Right: right,
	}
}

func tokenToOp(tok token.Token) string {
	switch tok {
	case token.EQL:
		return "eq"
	case token.NEQ:
		return "ne"
	case token.LSS:
		return "lt"
	case token.GTR:
		return "gt"
	case token.LEQ:
		return "le"
	case token.GEQ:
		return "ge"
	case token.ADD:
		return "add"
	case token.SUB:
		return "sub"
	case token.MUL:
		return "mul"
	case token.QUO:
		return "div"
	case token.REM:
		return "mod"
	case token.LAND:
		return "and"
	case token.LOR:
		return "or"
	case token.NOT:
		return "not"
	default:
		return tok.String()
	}
}

// --- Dependency Detection ---

func extractDependencies(fset *token.FileSet, body *ast.BlockStmt, info *types.Info) []ExternalDependency {
	deps := map[string]*ExternalDependency{}

	ast.Inspect(body, func(n ast.Node) bool {
		call, ok := n.(*ast.CallExpr)
		if !ok {
			return true
		}

		sel, ok := call.Fun.(*ast.SelectorExpr)
		if !ok {
			return true
		}

		pkgIdent, ok := sel.X.(*ast.Ident)
		if !ok {
			return true
		}

		// Check if the identifier refers to an imported package
		obj := info.Uses[pkgIdent]
		if obj == nil {
			return true
		}
		pkgName, ok := obj.(*types.PkgName)
		if !ok {
			return true
		}

		symbol := pkgIdent.Name + "." + sel.Sel.Name
		pkgPath := pkgName.Imported().Path()
		line := fset.Position(call.Pos()).Line

		if existing, found := deps[symbol]; found {
			existing.CallSites = append(existing.CallSites, line)
			return true
		}

		dep := &ExternalDependency{
			Kind:         "function_call",
			Symbol:       symbol,
			SourceModule: pkgPath,
			ReturnType:   TypeInfo{Kind: "unknown"},
			ParamTypes:   []TypeInfo{},
			CallSites:    []int{line},
		}

		// Try to extract return type and param types from type info
		if fnObj := info.Uses[sel.Sel]; fnObj != nil {
			if sig, ok := fnObj.Type().(*types.Signature); ok {
				dep.ReturnType = sigReturnType(sig)
				dep.ParamTypes = sigParamTypes(sig)
			}
		}

		deps[symbol] = dep
		return true
	})

	result := make([]ExternalDependency, 0, len(deps))
	for _, d := range deps {
		result = append(result, *d)
	}
	return result
}

func sigReturnType(sig *types.Signature) TypeInfo {
	results := sig.Results()
	if results == nil || results.Len() == 0 {
		return TypeInfo{Kind: "unknown"}
	}
	if results.Len() == 1 {
		return goTypeToTypeInfo(results.At(0).Type())
	}
	fields := make([]ObjectField, results.Len())
	for i := 0; i < results.Len(); i++ {
		v := results.At(i)
		name := v.Name()
		if name == "" {
			name = fmt.Sprintf("_%d", i)
		}
		fields[i] = ObjectField{
			Name: name,
			Type: goTypeToTypeInfo(v.Type()),
		}
	}
	return TypeInfo{Kind: "object", Fields: fields}
}

func sigParamTypes(sig *types.Signature) []TypeInfo {
	params := sig.Params()
	if params == nil || params.Len() == 0 {
		return []TypeInfo{}
	}
	result := make([]TypeInfo, params.Len())
	for i := 0; i < params.Len(); i++ {
		result[i] = goTypeToTypeInfo(params.At(i).Type())
	}
	return result
}

// --- Utilities ---

func exprText(fset *token.FileSet, expr ast.Expr) string {
	var buf strings.Builder
	printer.Fprint(&buf, fset, expr)
	return buf.String()
}

func exprString(expr ast.Expr) string {
	var buf strings.Builder
	printer.Fprint(&buf, token.NewFileSet(), expr)
	return buf.String()
}
