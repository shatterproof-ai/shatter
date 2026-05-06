// Package wrapper generates cache-stable Go wrapper files for discovered targets.
//
// Each wrapper exports PlanDescriptor and ShatterInvoke so that an external
// orchestrator can specify an invocation strategy (receiver construction +
// parameter source) and execute it without recompiling the target module.
// The file name embeds a discovery hash that is stable as long as the set of
// targets and constructors does not change; rebuilds are skipped when the hash
// matches an already-generated file.
package wrapper

import (
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"go/ast"
	"go/printer"
	"go/token"
	"go/types"
	"os"
	"sort"
	"strings"

	"golang.org/x/tools/go/packages"
)

// WrapperParam describes one parameter of a wrapper target, including the
// Go type name required for code generation.
//
// IsVariadic is true only for the final positional parameter of a function
// declared with `...T`. The GoType for a variadic parameter is the slice
// form (`[]T`); the call site must expand it with `args...` so the wrapper
// passes through to the target's variadic call shape (str-jeen.48).
type WrapperParam struct {
	Name       string
	GoType     string // concrete Go type string, e.g. "int", "*Counter", "string"
	IsVariadic bool
}

// TypeParamInfo describes one generic type parameter declared by a wrapper target.
type TypeParamInfo struct {
	Name       string
	Constraint string
}

// WrapperTarget is an enriched description of a discovered invocation target
// with Go-level type information for code generation.
type WrapperTarget struct {
	ID            string // stable target ID, e.g. "example.com/pkg:Add"
	SymbolName    string // bare function or method name
	Kind          TargetKind
	ReceiverType  string // bare type name (without *) for method targets
	IsPointerRecv bool   // true for (*T).Method receivers
	Parameters    []WrapperParam
	TypeParams    []TypeParamInfo
	HasResult     bool
	ResultGoType  string // Go type string for the first return value
	ResultCount   int    // total number of return values (0 when HasResult is false)
	// Imports lists the import paths required by qualified type names that the
	// generated wrapper source actually references. Today that means parameter
	// types such as `context.Context`, `*pgx.Conn`, or `gqlerror.Error`; result
	// types are tracked in ResultGoType for metadata, but the generated source
	// does not name them and must not import result-only packages.
	// Cross-ref: str-jeen.33 and str-iylc.
	Imports []string
}

const (
	// WrapperKindZeroValue selects zero-value receiver construction.
	WrapperKindZeroValue = "zero_value"
	// WrapperKindConstructorPrefix is prepended to a constructor function
	// name to form the ReceiverKind string.
	WrapperKindConstructorPrefix = "constructor:"
)

// DiscoveryHash returns a 16-character hex prefix of the SHA-256 over the
// sorted target IDs, target type parameter signatures, target imports, and
// sorted constructor function metadata. The hash is fully determined by the
// discovery results so the wrapper filename is stable for the same set of
// targets and constructors, and changes when newly discovered imports would
// otherwise require regenerating an existing wrapper file.
func DiscoveryHash(targets []WrapperTarget, constructors []ConstructorCandidate) string {
	ids := make([]string, len(targets))
	for i, t := range targets {
		ids[i] = t.ID + ":" + typeParamSignature(t.TypeParams) + ":" + strings.Join(sortedStrings(t.Imports), ",")
	}
	sort.Strings(ids)

	ctors := make([]string, len(constructors))
	for i, c := range constructors {
		hasParams := "0"
		if c.HasParams {
			hasParams = "1"
		}
		// str-jeen.49: include ReturnsPointer in the hash so that a
		// constructor whose return kind changes invalidates the cached
		// wrapper file and triggers regeneration with the correct
		// dereference shape.
		returnsPtr := "0"
		if c.ReturnsPointer {
			returnsPtr = "1"
		}
		ctors[i] = c.FuncName + ":" + c.TargetType + ":" + hasParams + ":" + returnsPtr
	}
	sort.Strings(ctors)

	payload := strings.Join(ids, "\n") + "\n---\n" + strings.Join(ctors, "\n")
	sum := sha256.Sum256([]byte(payload))
	return hex.EncodeToString(sum[:])[:16]
}

// WrapperFilename returns the conventional filename for a wrapper given its
// discovery hash.
func WrapperFilename(hash string) string {
	return fmt.Sprintf("shatter_wrapper_%s.go", hash)
}

// GenerateWrapper produces a deterministic Go source file that exports
// PlanDescriptor and ShatterInvoke for the given targets and constructors.
// The file declares package pkgName.
//
// The output is fully determined by the inputs; calling GenerateWrapper twice
// with identical arguments produces byte-identical output.
func GenerateWrapper(
	pkgName string,
	targets []WrapperTarget,
	constructors []ConstructorCandidate,
) string {
	// Sort targets and constructors for determinism.
	sorted := make([]WrapperTarget, len(targets))
	copy(sorted, targets)
	sort.Slice(sorted, func(i, j int) bool { return sorted[i].ID < sorted[j].ID })

	sortedCtors := make([]ConstructorCandidate, len(constructors))
	copy(sortedCtors, constructors)
	sort.Slice(sortedCtors, func(i, j int) bool { return sortedCtors[i].FuncName < sortedCtors[j].FuncName })

	// Index constructors by target type for receiver-kind enumeration.
	// Skip constructors whose real signature takes parameters: the wrapper
	// has no way to synthesise the arguments, and emitting `_recv :=
	// NewFoo()` against a constructor that requires (e.g.) an
	// http.ResponseWriter produces a package-wide build error that
	// silently poisons every other target's wrapper case (str-qo1.14).
	ctorsByType := make(map[string][]ConstructorCandidate)
	for _, c := range sortedCtors {
		if c.HasParams {
			continue
		}
		ctorsByType[c.TargetType] = append(ctorsByType[c.TargetType], c)
	}

	var b strings.Builder

	fmt.Fprintf(&b, "package %s\n\n", pkgName)
	b.WriteString("// Code generated by Shatter. DO NOT EDIT.\n\n")
	b.WriteString("import (\n")
	b.WriteString("\t\"encoding/json\"\n")
	b.WriteString("\t\"fmt\"\n")
	if hasGenericTargets(sorted) {
		b.WriteString("\t\"strings\"\n")
	}
	// str-jeen.33: union the per-target Imports lists and emit one entry per
	// distinct import path. Without this, qualified parameter or return types
	// like context.Context, *pgx.Conn, slog.Logger would leave the generated
	// wrapper file referencing undefined package short names.
	for _, importPath := range collectExtraImports(sorted) {
		fmt.Fprintf(&b, "\t%q\n", importPath)
	}
	b.WriteString(")\n\n")

	b.WriteString("// PlanDescriptor selects one invocation strategy for one ShatterInvoke call.\n")
	b.WriteString("type PlanDescriptor struct {\n")
	b.WriteString("\tTargetID     string `json:\"target_id\"`\n")
	b.WriteString("\tReceiverKind string `json:\"receiver_kind\"`\n")
	b.WriteString("\tGenericTypeArgs []string `json:\"generic_type_args,omitempty\"`\n")
	b.WriteString("}\n\n")

	b.WriteString("// ShatterInvoke executes the strategy in d against inputs and returns the result.\n")
	b.WriteString("func ShatterInvoke(d PlanDescriptor, inputs []json.RawMessage) (any, error) {\n")
	b.WriteString("\tswitch d.TargetID {\n")

	for _, t := range sorted {
		fmt.Fprintf(&b, "\tcase %q:\n", t.ID)
		writeTargetCase(&b, t, ctorsByType)
	}

	b.WriteString("\t}\n")
	b.WriteString("\treturn nil, fmt.Errorf(\"shatter: unknown target: %s\", d.TargetID)\n")
	b.WriteString("}\n")

	return b.String()
}

func writeTargetCase(b *strings.Builder, t WrapperTarget, ctorsByType map[string][]ConstructorCandidate) {
	if len(t.TypeParams) > 0 {
		writeGenericTargetCase(b, t)
		return
	}
	if t.Kind == TargetKindFunction {
		writeParamDeserialization(b, t.Parameters, "\t\t")
		writeCall(b, t, "", nil, "\t\t")
		return
	}

	b.WriteString("\t\tswitch d.ReceiverKind {\n")

	b.WriteString("\t\tcase \"zero_value\":\n")
	if t.IsPointerRecv {
		fmt.Fprintf(b, "\t\t\tvar _recvVal %s\n", t.ReceiverType)
		b.WriteString("\t\t\t_recv := &_recvVal\n")
	} else {
		fmt.Fprintf(b, "\t\t\tvar _recv %s\n", t.ReceiverType)
	}
	writeParamDeserialization(b, t.Parameters, "\t\t\t")
	writeCall(b, t, "_recv", nil, "\t\t\t")

	if ctors, ok := ctorsByType[t.ReceiverType]; ok {
		for _, c := range ctors {
			recvKind := WrapperKindConstructorPrefix + c.FuncName
			fmt.Fprintf(b, "\t\tcase %q:\n", recvKind)
			// str-jeen.49: choose the call shape from the cross product
			// of (target receiver kind) × (constructor return kind).
			// Pre-fix every value-receiver case dereferenced the
			// constructor result, which fails to compile when the
			// constructor returns the value form (`cannot indirect`).
			switch {
			case t.IsPointerRecv && c.ReturnsPointer:
				fmt.Fprintf(b, "\t\t\t_recv := %s()\n", c.FuncName)
			case t.IsPointerRecv && !c.ReturnsPointer:
				fmt.Fprintf(b, "\t\t\t_recvVal := %s()\n", c.FuncName)
				b.WriteString("\t\t\t_recv := &_recvVal\n")
			case !t.IsPointerRecv && c.ReturnsPointer:
				fmt.Fprintf(b, "\t\t\t_recv := *%s()\n", c.FuncName)
			default: // !t.IsPointerRecv && !c.ReturnsPointer
				fmt.Fprintf(b, "\t\t\t_recv := %s()\n", c.FuncName)
			}
			writeParamDeserialization(b, t.Parameters, "\t\t\t")
			writeCall(b, t, "_recv", nil, "\t\t\t")
		}
	}

	b.WriteString("\t\t}\n")
	fmt.Fprintf(b, "\t\treturn nil, fmt.Errorf(\"shatter: unknown receiver kind for %s: %%s\", d.ReceiverKind)\n", t.ID)
}

func writeParamDeserialization(b *strings.Builder, params []WrapperParam, indent string) {
	for i, p := range params {
		fmt.Fprintf(b, "%svar %s %s\n", indent, p.Name, p.GoType)
		fmt.Fprintf(b, "%sif %d < len(inputs) {\n", indent, i)
		fmt.Fprintf(b, "%s\tif _e := json.Unmarshal(inputs[%d], &%s); _e != nil {\n", indent, i, p.Name)
		fmt.Fprintf(b, "%s\t\treturn nil, fmt.Errorf(\"param %s: %%w\", _e)\n", indent, p.Name)
		fmt.Fprintf(b, "%s\t}\n", indent)
		fmt.Fprintf(b, "%s}\n", indent)
	}
}

func writeGenericTargetCase(b *strings.Builder, t WrapperTarget) {
	if t.Kind != TargetKindFunction {
		fmt.Fprintf(b, "\t\treturn nil, fmt.Errorf(\"shatter: generic method targets are not supported: %s\")\n", t.ID)
		return
	}
	combos := wrapperGenericTypeArgSets(t.TypeParams)
	b.WriteString("\t\tswitch strings.Join(d.GenericTypeArgs, \",\") {\n")
	for _, combo := range combos {
		key := strings.Join(combo, ",")
		fmt.Fprintf(b, "\t\tcase %q:\n", key)
		writeParamDeserializationWithTypeArgs(b, t.Parameters, t.TypeParams, combo, "\t\t\t")
		writeCall(b, t, "", combo, "\t\t\t")
	}
	b.WriteString("\t\t}\n")
	fmt.Fprintf(b, "\t\treturn nil, fmt.Errorf(\"shatter: unsupported generic type args for %s: %%v\", d.GenericTypeArgs)\n", t.ID)
}

func writeCall(b *strings.Builder, t WrapperTarget, recvExpr string, typeArgs []string, indent string) {
	args := make([]string, len(t.Parameters))
	for i, p := range t.Parameters {
		// str-jeen.48: a variadic parameter (declared `...T` in source) is
		// stored as a slice but must be expanded at the call site so the
		// wrapper produces the same call shape as the target's signature.
		// Without `args...` the build fails with `cannot use args (variable
		// of type []T) as T`.
		if p.IsVariadic {
			args[i] = p.Name + "..."
		} else {
			args[i] = p.Name
		}
	}
	argList := strings.Join(args, ", ")

	var callExpr string
	symbolName := t.SymbolName
	if len(typeArgs) > 0 {
		symbolName += "[" + strings.Join(typeArgs, ", ") + "]"
	}
	if recvExpr == "" {
		callExpr = fmt.Sprintf("%s(%s)", symbolName, argList)
	} else {
		callExpr = fmt.Sprintf("%s.%s(%s)", recvExpr, symbolName, argList)
	}

	if t.HasResult {
		if t.ResultCount > 1 {
			blanks := strings.Repeat(", _", t.ResultCount-1)
			fmt.Fprintf(b, "%s_result%s := %s\n", indent, blanks, callExpr)
		} else {
			fmt.Fprintf(b, "%s_result := %s\n", indent, callExpr)
		}
		fmt.Fprintf(b, "%sreturn _result, nil\n", indent)
	} else {
		fmt.Fprintf(b, "%s%s\n", indent, callExpr)
		fmt.Fprintf(b, "%sreturn nil, nil\n", indent)
	}
}

// BuildWrapperTargets extracts a WrapperTarget for every function in pkg
// (both free functions and methods).
//
// Synthetic package init functions are excluded (str-qo1.8). Go forbids
// calling `init` directly, and a package may declare multiple `init`
// functions across files; emitting them would both produce uncompilable
// `init()` call sites and collide on a single switch case for
// "<pkg>:init", making the wrapper file uncompilable.
func BuildWrapperTargets(pkg *packages.Package) []WrapperTarget {
	if pkg == nil || pkg.TypesInfo == nil {
		return nil
	}
	var targets []WrapperTarget
	for _, file := range pkg.Syntax {
		if file == nil {
			continue
		}
		for _, decl := range file.Decls {
			fn, ok := decl.(*ast.FuncDecl)
			if !ok || fn.Body == nil {
				continue
			}
			if isSyntheticPackageInit(fn) {
				continue
			}
			if t := buildWrapperTarget(fn, pkg); t != nil {
				targets = append(targets, *t)
			}
		}
	}
	return targets
}

// isSyntheticPackageInit reports whether fn is a Go package-init function
// (`func init()` at package scope with no receiver). These cannot be invoked
// directly as targets — see BuildWrapperTargets and AnalyzeFile (str-qo1.8).
func isSyntheticPackageInit(fn *ast.FuncDecl) bool {
	if fn == nil || fn.Name == nil {
		return false
	}
	if fn.Recv != nil && len(fn.Recv.List) > 0 {
		return false
	}
	return fn.Name.Name == "init"
}

func buildWrapperTarget(fn *ast.FuncDecl, pkg *packages.Package) *WrapperTarget {
	qualName := wrapperQualifiedName(fn)
	id := pkg.PkgPath + ":" + qualName

	kind := TargetKindFunction
	var recvType string
	var isPtr bool

	if fn.Recv != nil && len(fn.Recv.List) > 0 {
		kind = TargetKindMethod
		expr := fn.Recv.List[0].Type
		if star, ok := expr.(*ast.StarExpr); ok {
			isPtr = true
			if ident, ok := star.X.(*ast.Ident); ok {
				recvType = ident.Name
			}
		} else if ident, ok := expr.(*ast.Ident); ok {
			recvType = ident.Name
		}
	}

	// importSet accumulates every import path referenced by parameter type
	// expressions on this function so wrapper-gen can emit matching import
	// statements (str-jeen.33). Result-only imports are intentionally omitted
	// because the generated wrapper does not name result types (str-iylc).
	importSet := make(map[string]struct{})
	params := extractWrapperParams(fn, pkg.TypesInfo, pkg.Name, importSet)
	typeParams := extractWrapperTypeParams(fn)

	hasResult := false
	var resultGoType string
	resultCount := 0
	if fn.Type.Results != nil && len(fn.Type.Results.List) > 0 {
		hasResult = true
		resultGoType = wrapperGoType(fn.Type.Results.List[0].Type, pkg.TypesInfo, pkg.Name, nil)
		for _, field := range fn.Type.Results.List {
			if len(field.Names) == 0 {
				resultCount++
			} else {
				resultCount += len(field.Names)
			}
		}
	}

	imports := make([]string, 0, len(importSet))
	for importPath := range importSet {
		imports = append(imports, importPath)
	}
	sort.Strings(imports)

	return &WrapperTarget{
		ID:            id,
		SymbolName:    fn.Name.Name,
		Kind:          kind,
		ReceiverType:  recvType,
		IsPointerRecv: isPtr,
		Parameters:    params,
		TypeParams:    typeParams,
		HasResult:     hasResult,
		ResultGoType:  resultGoType,
		ResultCount:   resultCount,
		Imports:       imports,
	}
}

func sortedStrings(values []string) []string {
	if len(values) == 0 {
		return nil
	}
	out := append([]string{}, values...)
	sort.Strings(out)
	return out
}

func wrapperQualifiedName(fn *ast.FuncDecl) string {
	if fn.Recv == nil || len(fn.Recv.List) == 0 {
		return fn.Name.Name
	}
	expr := fn.Recv.List[0].Type
	if star, ok := expr.(*ast.StarExpr); ok {
		if ident, ok := star.X.(*ast.Ident); ok {
			return "(*" + ident.Name + ")." + fn.Name.Name
		}
	}
	if ident, ok := expr.(*ast.Ident); ok {
		return "(" + ident.Name + ")." + fn.Name.Name
	}
	return fn.Name.Name
}

// syntheticParamPrefix is the prefix used for generated parameter local
// names when the source signature does not provide a usable identifier
// (either no name at all, e.g. `func F(int, string)`, or the blank
// identifier `_`, e.g. `func F(_ int, _ string)`). The wrapper later
// references each parameter local in `json.Unmarshal(&p)` and in the
// call expression, so emitting `_` would produce uncompilable code
// ("cannot use _ as value or type"). See str-qo1.7.
const syntheticParamPrefix = "_p"

// syntheticParamName returns the stable wrapper-local name for the
// parameter at position index. The index is the parameter's position in
// the flattened list (each name in a `(a, b int)` field counts as one
// position). The prefix is fixed by syntheticParamPrefix so generated
// names are byte-stable across calls and never collide with idiomatic
// Go identifiers.
func syntheticParamName(index int) string {
	return fmt.Sprintf("%s%d", syntheticParamPrefix, index)
}

func extractWrapperParams(fn *ast.FuncDecl, info *types.Info, pkgName string, importSet map[string]struct{}) []WrapperParam {
	if fn.Type.Params == nil {
		return nil
	}
	var params []WrapperParam
	index := 0
	for _, field := range fn.Type.Params.List {
		// str-jeen.48: detect a `...T` parameter. Go's grammar permits the
		// ellipsis only on the final field, and that field always carries
		// at most one named identifier. The local variable type is `[]T`,
		// not `...T`; IsVariadic drives `args...` expansion at the call
		// site (see writeCall).
		fieldType := field.Type
		isVariadic := false
		if ellipsis, ok := fieldType.(*ast.Ellipsis); ok {
			isVariadic = true
			fieldType = ellipsis.Elt
		}
		elemType := wrapperGoType(fieldType, info, pkgName, importSet)
		goType := elemType
		if isVariadic {
			goType = "[]" + elemType
		}
		if len(field.Names) == 0 {
			// Unnamed parameter (e.g. `func F(int, string)`): a single
			// field with no names represents one positional parameter.
			params = append(params, WrapperParam{Name: syntheticParamName(index), GoType: goType, IsVariadic: isVariadic})
			index++
			continue
		}
		for _, name := range field.Names {
			localName := name.Name
			if localName == "" || localName == "_" {
				// Blank-identifier parameter (e.g. `func F(_ int)`):
				// the source name is unusable as a wrapper local, so
				// substitute a stable synthetic name.
				localName = syntheticParamName(index)
			}
			params = append(params, WrapperParam{Name: localName, GoType: goType, IsVariadic: isVariadic})
			index++
		}
	}
	return params
}

func extractWrapperTypeParams(fn *ast.FuncDecl) []TypeParamInfo {
	if fn.Type.TypeParams == nil || len(fn.Type.TypeParams.List) == 0 {
		return nil
	}
	var params []TypeParamInfo
	for _, field := range fn.Type.TypeParams.List {
		constraint := "any"
		if field.Type != nil {
			constraint = strings.TrimSpace(wrapperASTExprString(field.Type))
			if constraint == "" {
				constraint = "any"
			}
		}
		for _, name := range field.Names {
			params = append(params, TypeParamInfo{Name: name.Name, Constraint: constraint})
		}
	}
	return params
}

// wrapperGoType returns the Go type string for use in the target package and,
// as a side effect, records every external package referenced by the type
// into importSet (keyed by import path). importSet may be nil. Cross-ref:
// str-jeen.33 — without this, the generated wrapper file declares variables
// of qualified types (`context.Context`, `*pgx.Conn`) without ever importing
// the corresponding packages.
//
// str-qo1.13: when info.Types lacks an entry for expr (e.g. because the
// caller initialized only Defs/Uses, or because the type checker did not
// record this particular type expression), the function still walks the AST
// for *ast.SelectorExpr nodes and consults info.Uses to recover package
// imports. Without this, selector type expressions (e.g. http.ResponseWriter)
// would be printed by wrapperASTTypeString verbatim while the corresponding
// package import (`net/http`) was silently dropped, producing a wrapper that
// references an undefined package short name and fails to compile.
func wrapperGoType(expr ast.Expr, info *types.Info, pkgName string, importSet map[string]struct{}) string {
	if info != nil {
		if tv, ok := info.Types[expr]; ok && tv.Type != nil {
			qualifier := func(p *types.Package) string {
				if p == nil || p.Name() == pkgName {
					return ""
				}
				if importSet != nil {
					importSet[p.Path()] = struct{}{}
				}
				return p.Name()
			}
			return types.TypeString(tv.Type, qualifier)
		}
	}
	if info != nil && importSet != nil {
		collectSelectorImports(expr, info, importSet)
	}
	return wrapperASTTypeString(expr)
}

// collectSelectorImports walks expr looking for selector-type expressions
// (`pkg.Type`) and, for each one, records the import path of the imported
// package into importSet via info.Uses. It is the AST-fallback complement to
// the qualifier-driven import collection performed when info.Types is
// populated. Cross-ref: str-qo1.13.
func collectSelectorImports(expr ast.Expr, info *types.Info, importSet map[string]struct{}) {
	if expr == nil || info == nil || importSet == nil {
		return
	}
	ast.Inspect(expr, func(n ast.Node) bool {
		sel, ok := n.(*ast.SelectorExpr)
		if !ok {
			return true
		}
		ident, ok := sel.X.(*ast.Ident)
		if !ok {
			return true
		}
		obj := info.Uses[ident]
		if obj == nil {
			obj = info.Defs[ident]
		}
		pkgName, ok := obj.(*types.PkgName)
		if !ok || pkgName == nil {
			return true
		}
		imported := pkgName.Imported()
		if imported == nil {
			return true
		}
		importSet[imported.Path()] = struct{}{}
		return true
	})
}

func wrapperASTExprString(expr ast.Expr) string {
	var b strings.Builder
	if err := printer.Fprint(&b, token.NewFileSet(), expr); err != nil {
		return ""
	}
	return b.String()
}

func wrapperASTTypeString(expr ast.Expr) string {
	switch e := expr.(type) {
	case *ast.Ident:
		return e.Name
	case *ast.StarExpr:
		return "*" + wrapperASTTypeString(e.X)
	case *ast.ArrayType:
		if e.Len == nil {
			return "[]" + wrapperASTTypeString(e.Elt)
		}
		return "[...]" + wrapperASTTypeString(e.Elt)
	case *ast.Ellipsis:
		// str-jeen.48: a `...T` parameter type renders as `[]T` for the
		// wrapper's local variable; the call site appends `...` based on
		// IsVariadic, not on the rendered type string.
		return "[]" + wrapperASTTypeString(e.Elt)
	case *ast.MapType:
		return "map[" + wrapperASTTypeString(e.Key) + "]" + wrapperASTTypeString(e.Value)
	case *ast.SelectorExpr:
		return wrapperASTTypeString(e.X) + "." + e.Sel.Name
	case *ast.InterfaceType:
		return "any"
	default:
		return "any"
	}
}

// collectExtraImports returns the sorted union of all targets' Imports lists,
// excluding the always-emitted core imports (encoding/json, fmt, strings) so
// they are never duplicated. The output is deterministic so GenerateWrapper
// remains byte-stable across calls. See str-jeen.33.
func collectExtraImports(targets []WrapperTarget) []string {
	const (
		coreImportJSON    = "encoding/json"
		coreImportFmt     = "fmt"
		coreImportStrings = "strings"
	)
	seen := make(map[string]struct{})
	for _, t := range targets {
		for _, importPath := range t.Imports {
			trimmed := strings.TrimSpace(importPath)
			if trimmed == "" {
				continue
			}
			switch trimmed {
			case coreImportJSON, coreImportFmt, coreImportStrings:
				continue
			}
			seen[trimmed] = struct{}{}
		}
	}
	result := make([]string, 0, len(seen))
	for importPath := range seen {
		result = append(result, importPath)
	}
	sort.Strings(result)
	return result
}

func hasGenericTargets(targets []WrapperTarget) bool {
	for _, target := range targets {
		if len(target.TypeParams) > 0 {
			return true
		}
	}
	return false
}

func typeParamSignature(params []TypeParamInfo) string {
	if len(params) == 0 {
		return ""
	}
	parts := make([]string, len(params))
	for i, param := range params {
		parts[i] = param.Name + "=" + param.Constraint
	}
	return strings.Join(parts, ",")
}

func writeParamDeserializationWithTypeArgs(
	b *strings.Builder,
	params []WrapperParam,
	typeParams []TypeParamInfo,
	typeArgs []string,
	indent string,
) {
	subst := make(map[string]string, len(typeParams))
	for i, param := range typeParams {
		if i < len(typeArgs) {
			subst[param.Name] = typeArgs[i]
		}
	}
	resolved := make([]WrapperParam, len(params))
	for i, param := range params {
		resolved[i] = param
		if typeArg, ok := subst[param.GoType]; ok {
			resolved[i].GoType = typeArg
		}
	}
	writeParamDeserialization(b, resolved, indent)
}

func wrapperGenericTypeArgSets(params []TypeParamInfo) [][]string {
	sets := [][]string{{}}
	for _, param := range params {
		defaults := wrapperGenericDefaults(param.Constraint)
		next := make([][]string, 0, len(sets)*len(defaults))
		for _, prefix := range sets {
			for _, def := range defaults {
				next = append(next, append(append([]string{}, prefix...), def))
			}
		}
		sets = next
	}
	return sets
}

func wrapperGenericDefaults(constraint string) []string {
	switch strings.TrimSpace(constraint) {
	case "", "any", "interface{}", "comparable":
		return []string{"string", "int", "bool", "int64", "float64"}
	case "cmp.Ordered", "constraints.Ordered":
		return []string{"string", "int", "int64", "float64"}
	default:
		return nil
	}
}

// WriteWrapperFile writes the generated wrapper to dir/<WrapperFilename(hash)>
// and returns the file path. If the file already exists (inferred from the
// deterministic filename), the write is skipped — this is the rebuild-skip
// guarantee when the discovery hash has not changed.
//
// Returns (path, true, nil) when a new file is written, or (path, false, nil)
// when the existing file is reused.
func WriteWrapperFile(
	dir string,
	pkgName string,
	targets []WrapperTarget,
	constructors []ConstructorCandidate,
) (filePath string, fresh bool, err error) {
	hash := DiscoveryHash(targets, constructors)
	name := WrapperFilename(hash)
	path := dir + "/" + name

	if _, statErr := os.Stat(path); statErr == nil {
		return path, false, nil
	}

	src := GenerateWrapper(pkgName, targets, constructors)
	tmp := path + ".tmp"
	if writeErr := os.WriteFile(tmp, []byte(src), 0o644); writeErr != nil {
		return "", false, fmt.Errorf("wrapper: write temp %q: %w", tmp, writeErr)
	}
	if renameErr := os.Rename(tmp, path); renameErr != nil {
		_ = os.Remove(tmp)
		return "", false, fmt.Errorf("wrapper: rename to %q: %w", path, renameErr)
	}
	return path, true, nil
}
