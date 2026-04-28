package protocol

import (
	"fmt"
	"go/ast"
	"go/printer"
	"go/token"
	"go/types"
	"os"
	"path/filepath"
	"strconv"
	"strings"

	goloader "github.com/shatter-dev/shatter/shatter-go/loader"
	frontendtiming "github.com/shatter-dev/shatter/shatter-go/timing"
	"github.com/shatter-dev/shatter/shatter-go/workspace"
	"golang.org/x/tools/go/packages"
)

// fileContext carries file-level metadata needed for static opacity heuristics.
type fileContext struct {
	// exportedFuncNames is the set of all exported top-level function names in the file.
	exportedFuncNames map[string]bool
	// implementors maps interface type names to the struct names that implement them.
	implementors map[string][]string
	// pkgPath is the import path of the package being analyzed.
	pkgPath string
}

// buildFileContext scans every file in the package to populate a fileContext
// with exported function names and interface implementors. Using the full
// package syntax (not just the target file) lets constructor suppression and
// interface-implementation lookups see siblings in multi-file packages.
func buildFileContext(pkgName string, syntaxFiles []*ast.File, info *types.Info) *fileContext {
	fc := &fileContext{
		exportedFuncNames: make(map[string]bool),
		implementors:      make(map[string][]string),
		pkgPath:           pkgName,
	}

	var interfaceDefs []*ast.TypeSpec
	var structDefs []*ast.TypeSpec
	for _, file := range syntaxFiles {
		if file == nil {
			continue
		}
		for _, decl := range file.Decls {
			switch d := decl.(type) {
			case *ast.FuncDecl:
				if ast.IsExported(d.Name.Name) {
					fc.exportedFuncNames[d.Name.Name] = true
				}
			case *ast.GenDecl:
				for _, spec := range d.Specs {
					ts, ok := spec.(*ast.TypeSpec)
					if !ok {
						continue
					}
					if !ast.IsExported(ts.Name.Name) {
						continue
					}
					switch ts.Type.(type) {
					case *ast.InterfaceType:
						interfaceDefs = append(interfaceDefs, ts)
					case *ast.StructType:
						structDefs = append(structDefs, ts)
					}
				}
			}
		}
	}

	for _, structSpec := range structDefs {
		structObj, ok := info.Defs[structSpec.Name]
		if !ok {
			continue
		}
		structType, ok := structObj.Type().(*types.Named)
		if !ok {
			continue
		}
		for _, ifaceSpec := range interfaceDefs {
			ifaceObj, ok := info.Defs[ifaceSpec.Name]
			if !ok {
				continue
			}
			ifaceNamed, ok := ifaceObj.Type().(*types.Named)
			if !ok {
				continue
			}
			iface, ok := ifaceNamed.Underlying().(*types.Interface)
			if !ok {
				continue
			}
			if types.Implements(structType, iface) ||
				types.Implements(types.NewPointer(structType), iface) {
				fc.implementors[ifaceSpec.Name.Name] = append(
					fc.implementors[ifaceSpec.Name.Name], structSpec.Name.Name,
				)
			}
		}
	}
	return fc
}

// detectStaticOpacity applies static analysis heuristics to detect opaque types
// in the analyzed package. Only named types from the package itself are checked
// (external package types are handled by isOpaqueGoType).
//
// Returns the static_opacity reason string and true if detected, or ("", false).
//
// NOTE: Go's type system makes all structs constructible via struct literals,
// so no_constructor is only applied to types with unexported fields AND no
// exported factory function. Interfaces are not currently flagged to avoid
// false positives with service/data interface ambiguity.
func detectStaticOpacity(named *types.Named, fc *fileContext) (string, bool) {
	if fc == nil {
		return "", false
	}
	obj := named.Obj()
	// Only analyze types from the current package
	pkg := obj.Pkg()
	if pkg == nil || pkg.Name() != fc.pkgPath {
		return "", false
	}

	// Do not apply heuristics to interfaces — Go interfaces are always satisfied
	// by struct literals or zero values (via interface{}), and the existing
	// protocol treats interface params as "unknown". Flagging them as opaque
	// would break existing tests and semantics.
	if types.IsInterface(named) {
		return "", false
	}

	// no_constructor: struct whose underlying type has ALL unexported fields AND no
	// New*/Create*/Open* exported factory function. All-unexported fields means
	// the zero value or struct literal cannot be meaningfully used from outside
	// the package (the fields can't be read or written).
	typeName := obj.Name()
	underlying, ok := named.Underlying().(*types.Struct)
	if !ok {
		return "", false
	}
	if underlying.NumFields() == 0 {
		return "", false
	}
	allUnexported := true
	for i := 0; i < underlying.NumFields(); i++ {
		if underlying.Field(i).Exported() {
			allUnexported = false
			break
		}
	}
	if !allUnexported {
		return "", false
	}

	newFuncs := []string{
		"New" + typeName,
		"Create" + typeName,
		"Open" + typeName,
	}
	for _, fn := range newFuncs {
		if fc.exportedFuncNames[fn] {
			return "", false
		}
	}
	return "no_constructor", true

	// NOTE: "transitively_opaque" is deliberately not implemented here.
	// It would require resolving the first parameter type of New*/Create*/Open*
	// functions and checking if that type is itself opaque — a recursive lookup
	// that adds complexity for limited gain. It can be added in a future pass if
	// real-world examples warrant it.
}

// AnalyzeFile parses a Go source file and returns analysis for all exported
// functions, or a single function if functionName is non-empty. The file is
// loaded through the packages-based loader so sibling files in the same
// package contribute type information.
func AnalyzeFile(filePath string, functionName string) ([]FunctionAnalysis, error) {
	return AnalyzeFileWithTiming(filePath, functionName, nil)
}

// AnalyzeFileWithTiming loads a Go source file via a transient loader and
// records phase timings when requested. Handlers that already own a workspace
// should construct a *goloader.Loader once and call
// AnalyzeFileWithLoaderAndTiming directly.
func AnalyzeFileWithTiming(filePath string, functionName string, timing *frontendtiming.Collector) ([]FunctionAnalysis, error) {
	ldr, cleanup, err := newTransientLoader()
	if err != nil {
		return nil, err
	}
	defer cleanup()
	return AnalyzeFileWithLoaderAndTiming(filePath, functionName, ldr, timing)
}

// AnalyzeFileWithLoaderAndTiming analyzes filePath using the provided loader.
// Dispatches to LoadPackage when the file lives inside a Go module directory
// (and no `testdata` segment appears between the module root and the file);
// otherwise materializes a synthetic module via LoadFile.
func AnalyzeFileWithLoaderAndTiming(filePath string, functionName string, ldr *goloader.Loader, timing *frontendtiming.Collector) ([]FunctionAnalysis, error) {
	if ldr == nil {
		return nil, fmt.Errorf("analyzer requires a loader")
	}

	absoluteFilePath, err := filepath.Abs(filePath)
	if err != nil {
		return nil, fmt.Errorf("normalize file path %q: %w", filePath, err)
	}

	// Build-tag exclusion check: files gated by //go:build directives that do
	// not match the analyzer's default build context (or filename suffixes
	// such as *_windows.go on linux) are excluded from pkg.Syntax by
	// go/packages. Detect that case up front and surface a typed error so the
	// handler can emit ErrNotSupported and the Rust core's batch_analyze
	// soft-skip path consumes it instead of aborting on a generic
	// ParseError. See str-8amu.
	//
	// Gated on shouldLoadAsPackage: only files that will be loaded inside
	// their real Go module are subject to build-tag exclusion. Files outside
	// a module — and files under testdata — fall through to LoadFileLenient,
	// which materializes a synthetic single-file module and intentionally
	// analyzes the file regardless of its tags. This keeps fixtures like
	// shatter-go/protocol/testdata/gin_handler.go (//go:build ignore)
	// analyzable from tests while still soft-skipping tag-gated files in
	// real-world packages such as kapow's api/ui/embed.go.
	if shouldLoadAsPackage(absoluteFilePath) {
		if excluded, constraintExpr := isBuildTagExcluded(absoluteFilePath); excluded {
			return nil, &BuildTagExcludedError{
				Path:       absoluteFilePath,
				Constraint: constraintExpr,
			}
		}
	}

	// Retain the historical "analyze.parse" / "analyze.typecheck" phase names
	// so existing timing consumers keep working. The loader bundles parse and
	// typecheck into one go/packages call; we split the timing phases to
	// preserve the external contract even though both land on load().
	finishParse := timing.Start("analyze.parse")
	pkg, err := loadPackageForAnalysis(ldr, absoluteFilePath)
	finishParse()
	if err != nil {
		return nil, fmt.Errorf("load: %w", err)
	}

	fset := pkg.Fset
	if fset == nil {
		return nil, fmt.Errorf("loader returned package with no FileSet")
	}
	file := findTargetSyntaxFile(pkg, absoluteFilePath)
	if file == nil {
		return nil, fmt.Errorf("target file %q not found in loaded package syntax", absoluteFilePath)
	}

	info := pkg.TypesInfo
	finishTypeCheck := timing.Start("analyze.typecheck")
	fc := buildFileContext(pkg.Name, pkg.Syntax, info)
	finishTypeCheck()

	var results []FunctionAnalysis
	finishWalk := timing.Start("analyze.walk")
	for _, decl := range file.Decls {
		fn, ok := decl.(*ast.FuncDecl)
		if !ok || fn.Body == nil {
			continue
		}
		if functionName != "" && fn.Name.Name != functionName {
			continue
		}
		analysis := analyzeFuncWithContext(fset, fn, info, file, fc)
		results = append(results, analysis)
	}
	finishWalk()

	// Post-processing: attach adapter hints from recognizers.
	if len(results) > 0 {
		httpHints := RecognizeNetHTTPHandlers(fset, file, info, results)
		ginHints := RecognizeGinHandlers(fset, file, info, results)
		for i := range results {
			if httpHints[i] != nil {
				results[i].AdapterHints = append(results[i].AdapterHints, *httpHints[i])
			}
			if ginHints[i] != nil {
				results[i].AdapterHints = append(results[i].AdapterHints, *ginHints[i])
			}
		}
		// Promote high-confidence hints to invocation model when not already set.
		for i := range results {
			if results[i].InvocationModel != nil {
				continue
			}
			for _, hint := range results[i].AdapterHints {
				if hint.Confidence == "high" {
					results[i].InvocationModel = &InvocationModel{
						Kind:            "adapter",
						AdapterID:       hint.Adapter.ID,
						SyntheticParams: syntheticParamsForAdapter(hint.Adapter.ID),
					}
					break
				}
			}
		}
	}

	if functionName != "" && len(results) == 0 {
		return nil, fmt.Errorf("function not found: %s", functionName)
	}
	return results, nil
}

// syntheticParamsForAdapter returns the synthetic parameter definitions for a
// known adapter ID, or nil for unknown adapters. Used during hint-to-model
// promotion so that auto-promoted InvocationModels carry their SyntheticParams.
func syntheticParamsForAdapter(adapterID string) []ParamInfo {
	switch adapterID {
	case HTTPHandlerAdapterID:
		return httpHandlerSyntheticParams()
	case GinAdapterID:
		return ginHandlerSyntheticParams()
	default:
		return nil
	}
}

// loadPackageForAnalysis chooses between package-rooted loading (for files in
// a real Go module) and synthetic-module loading (for standalone files and
// testdata directories). Both paths use the lenient loader variants so that
// unresolved third-party imports don't abort analysis — the recognizers and
// AST walkers tolerate partial type information, matching the historical
// permissive typechecker behavior.
func loadPackageForAnalysis(ldr *goloader.Loader, absoluteFilePath string) (*packages.Package, error) {
	if shouldLoadAsPackage(absoluteFilePath) {
		if pkg, err := ldr.LoadPackageLenient(filepath.Dir(absoluteFilePath)); err == nil {
			return pkg, nil
		}
		// Fall through to synthetic-module path if package load fails for any
		// reason (e.g., sibling package-name conflicts). LoadFile creates an
		// isolated single-file module that is guaranteed to be self-consistent.
	}
	return ldr.LoadFileLenient(absoluteFilePath)
}

// shouldLoadAsPackage returns true when the file's directory is inside a real
// Go module and no `testdata` segment separates it from the module root.
// `testdata` directories are ignored by the Go build system by convention, so
// they are treated as standalone files for loading purposes.
func shouldLoadAsPackage(absoluteFilePath string) bool {
	fileDir := filepath.Dir(absoluteFilePath)
	moduleRoot, found := findGoModuleRoot(fileDir)
	if !found {
		return false
	}
	rel, err := filepath.Rel(moduleRoot, fileDir)
	if err != nil {
		return false
	}
	if rel == "." {
		return true
	}
	for _, segment := range strings.Split(rel, string(filepath.Separator)) {
		if segment == "testdata" {
			return false
		}
	}
	return true
}

func findGoModuleRoot(startDir string) (string, bool) {
	current := startDir
	for {
		if _, err := os.Stat(filepath.Join(current, "go.mod")); err == nil {
			return current, true
		}
		parent := filepath.Dir(current)
		if parent == current {
			return "", false
		}
		current = parent
	}
}

// findTargetSyntaxFile locates the *ast.File in pkg.Syntax whose file name
// matches absoluteFilePath. Returns nil if no syntax file matches.
func findTargetSyntaxFile(pkg *packages.Package, absoluteFilePath string) *ast.File {
	for _, file := range pkg.Syntax {
		if file == nil {
			continue
		}
		position := pkg.Fset.Position(file.Pos())
		syntaxPath, err := filepath.Abs(position.Filename)
		if err != nil {
			syntaxPath = position.Filename
		}
		if syntaxPath == absoluteFilePath {
			return file
		}
		if filepath.Base(syntaxPath) == filepath.Base(absoluteFilePath) && len(pkg.Syntax) == 1 {
			// Synthetic-module path: materialized file lives at a different
			// absolute path from the original but keeps its base name, and the
			// synthetic module contains exactly one file.
			return file
		}
	}
	return nil
}

// newTransientLoader builds a throwaway loader for AnalyzeFile callers that
// do not provide a long-lived workspace. Honors SHATTER_GO_WORKSPACE_ROOT
// when set; otherwise creates (and removes on cleanup) a per-call tempdir.
func newTransientLoader() (*goloader.Loader, func(), error) {
	if strings.TrimSpace(os.Getenv(workspace.EnvironmentRootKey)) != "" {
		workspaceHandle, err := workspace.Initialize(workspace.ResolveOptions{})
		if err != nil {
			return nil, func() {}, fmt.Errorf("initialize workspace: %w", err)
		}
		ldr, err := goloader.New(workspaceHandle)
		if err != nil {
			return nil, func() {}, fmt.Errorf("construct loader: %w", err)
		}
		return ldr, func() {}, nil
	}

	tempRoot, err := os.MkdirTemp("", "shatter-go-analyzer-*")
	if err != nil {
		return nil, func() {}, fmt.Errorf("create temp workspace: %w", err)
	}
	workspaceHandle, err := workspace.Open(tempRoot)
	if err != nil {
		_ = os.RemoveAll(tempRoot)
		return nil, func() {}, fmt.Errorf("open temp workspace: %w", err)
	}
	ldr, err := goloader.New(workspaceHandle)
	if err != nil {
		_ = os.RemoveAll(tempRoot)
		return nil, func() {}, fmt.Errorf("construct loader: %w", err)
	}
	cleanup := func() {
		_ = os.RemoveAll(tempRoot)
	}
	return ldr, cleanup, nil
}

func analyzeFunc(fset *token.FileSet, fn *ast.FuncDecl, info *types.Info, file *ast.File) FunctionAnalysis {
	return analyzeFuncWithContext(fset, fn, info, file, nil)
}

func analyzeFuncWithContext(fset *token.FileSet, fn *ast.FuncDecl, info *types.Info, file *ast.File, fc *fileContext) FunctionAnalysis {
	params := extractParamsWithContext(fn, info, fc)
	returnType := extractReturnType(fn, info)
	paramNames := paramNameSet(params)
	branches := extractBranches(fset, fn.Body, paramNames)
	loops := extractLoops(fset, fn.Body, paramNames)
	deps := extractDependencies(fset, fn.Body, info)
	literals := extractLiterals(fn, file)

	startLine := fset.Position(fn.Pos()).Line
	endLine := fset.Position(fn.End()).Line

	analysis := FunctionAnalysis{
		Name:         fn.Name.Name,
		Exported:     ast.IsExported(fn.Name.Name),
		Params:       params,
		Branches:     branches,
		Dependencies: deps,
		ReturnType:   returnType,
		StartLine:    startLine,
		EndLine:      endLine,
		Literals:     literals,
		Loops:        loops,
	}

	// Adapter recognition: check if this function is a known handler pattern
	// and populate InvocationModel so the execute path dispatches through
	// an adapter hook instead of fabricating plain arguments.
	if model := recognizeHTTPHandler(fn, info); model != nil {
		analysis.InvocationModel = model
	}

	return analysis
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
	return extractParamsWithContext(fn, info, nil)
}

func extractParamsWithContext(fn *ast.FuncDecl, info *types.Info, fc *fileContext) []ParamInfo {
	if fn.Type.Params == nil {
		return []ParamInfo{}
	}
	var params []ParamInfo
	for _, field := range fn.Type.Params.List {
		ti := goTypeFromExprWithContext(field.Type, info, fc)
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
	return goTypeFromExprWithContext(expr, info, nil)
}

func goTypeFromExprWithContext(expr ast.Expr, info *types.Info, fc *fileContext) TypeInfo {
	if tv, ok := info.Types[expr]; ok {
		return goTypeToTypeInfoWithContext(tv.Type, fc)
	}
	// Fallback: infer from AST when type checker didn't resolve
	return typeInfoFromAST(expr)
}

// infraGoPackagePrefixes lists Go import path prefixes whose types are
// medium-confidence opaque (almost always hold external resources such as
// database connections, cloud clients, or message queues).
//
// Types from these prefixes that are not already in opaqueGoTypes are detected
// as medium-confidence opaque via isMediumOpaqueGoType.
var infraGoPackagePrefixes = []string{
	"cloud.google.com/go",
	"github.com/go-redis/",
	"github.com/jackc/pgx",
	"github.com/aws/aws-sdk-go",
	"go.mongodb.org/mongo-driver",
	"github.com/elastic/go-elasticsearch",
	"github.com/streadway/amqp",
	"github.com/nats-io/nats.go",
	"go.etcd.io/etcd/client",
	"github.com/segmentio/kafka-go",
	"github.com/confluentinc/confluent-kafka-go",
}

// nativeHandleFieldNames contains struct field names that suggest an OS handle.
var nativeHandleFieldNames = map[string]bool{
	"fd":             true,
	"Fd":             true,
	"handle":         true,
	"Handle":         true,
	"fileDescriptor": true,
	"FileDescriptor": true,
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

// isMediumOpaqueGoType checks whether t shows medium-confidence signals of being
// an opaque infrastructure resource type. Returns the label, reason string, and true
// if detected; or ("", "", false).
//
// Does not re-check types already covered by isOpaqueGoType (high-confidence).
//
// Medium-confidence signal: type is returned as kind:"opaque" with medium_opacity set,
// but check_executability in the Rust core does NOT skip based on this alone.
// This is advisory metadata for learning mode — see executability.rs for skip policy.
//
// Heuristics:
//  1. InfrastructurePackage: import path starts with a known infra prefix
//  2. CloseableInterface: type has a Close() method with no params and one return value
//  3. NativeHandleField: struct fields named fd/handle/FileDescriptor
func isMediumOpaqueGoType(t types.Type) (string, string, bool) {
	named, ok := t.(*types.Named)
	if !ok {
		return "", "", false
	}
	obj := named.Obj()
	pkg := obj.Pkg()
	if pkg == nil {
		return "", "", false
	}
	pkgPath := pkg.Path()
	label := pkg.Name() + "." + obj.Name()

	// Heuristic 1: infrastructure package prefix
	for _, prefix := range infraGoPackagePrefixes {
		if strings.HasPrefix(pkgPath, prefix) {
			return label, "infrastructure_package", true
		}
	}

	// Heuristic 2: has Close() error method (io.Closer-like)
	if hasCloseMethod(named) {
		return label, "closeable_interface", true
	}

	// Heuristic 3: struct with native handle fields
	if hasNativeHandleField(named) {
		return label, "native_handle_field", true
	}

	return "", "", false
}

// hasCloseMethod reports whether the named type (or its pointer) has an exported
// Close method with no parameters and a single return value.
//
// Methods with pointer receivers (e.g. func (m *T) Close() error) are associated
// with the pointer type in Go's type system, so both the value type and its pointer
// are checked.
func hasCloseMethod(named *types.Named) bool {
	// Check value-receiver methods
	for i := 0; i < named.NumMethods(); i++ {
		m := named.Method(i)
		if m.Name() != "Close" {
			continue
		}
		sig, ok := m.Type().(*types.Signature)
		if !ok {
			continue
		}
		if sig.Params().Len() == 0 && sig.Results().Len() == 1 {
			return true
		}
	}
	// Check pointer-receiver methods via the pointer type's method set
	ptrType := types.NewPointer(named)
	ms := types.NewMethodSet(ptrType)
	for i := 0; i < ms.Len(); i++ {
		sel := ms.At(i)
		if sel.Obj().Name() != "Close" {
			continue
		}
		sig, ok := sel.Type().(*types.Signature)
		if !ok {
			continue
		}
		if sig.Params().Len() == 0 && sig.Results().Len() == 1 {
			return true
		}
	}
	return false
}

// hasNativeHandleField reports whether the named type's underlying struct has
// a field whose name suggests an OS handle (fd, handle, FileDescriptor) or
// whose type is unsafe.Pointer or uintptr.
func hasNativeHandleField(named *types.Named) bool {
	underlying, ok := named.Underlying().(*types.Struct)
	if !ok {
		return false
	}
	for i := 0; i < underlying.NumFields(); i++ {
		field := underlying.Field(i)
		if nativeHandleFieldNames[field.Name()] {
			return true
		}
		// Check for unsafe.Pointer or uintptr field type
		switch field.Type().String() {
		case "unsafe.Pointer", "uintptr":
			return true
		}
	}
	return false
}

func goTypeToTypeInfo(t types.Type) TypeInfo {
	return goTypeToTypeInfoRec(t, nil, make(map[types.Type]bool))
}

func goTypeToTypeInfoWithContext(t types.Type, fc *fileContext) TypeInfo {
	return goTypeToTypeInfoRec(t, fc, make(map[types.Type]bool))
}

func goTypeToTypeInfoRec(t types.Type, fc *fileContext, visited map[types.Type]bool) TypeInfo {
	// Cycle detection: if we've already started resolving this type, return a
	// stub to break the infinite recursion (e.g., type A struct { B *B } where
	// B struct { A *A }).
	if visited[t] {
		return TypeInfo{Kind: "object", Label: t.String()}
	}
	visited[t] = true
	defer delete(visited, t)

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
	// Static analysis heuristics for named types from the analyzed package
	// (checked before medium-confidence heuristics — higher confidence takes priority)
	if fc != nil {
		if named, ok := t.(*types.Named); ok {
			if reason, detected := detectStaticOpacity(named, fc); detected {
				pkg := named.Obj().Pkg()
				var label string
				if pkg != nil {
					label = pkg.Name() + "." + named.Obj().Name()
				} else {
					label = named.Obj().Name()
				}
				return TypeInfo{Kind: "opaque", Label: label, StaticOpacity: reason}
			}
		}
	}
	// Medium-confidence opaque detection: infra package prefix, Closeable, native handles.
	// Checked after static analysis so that high-confidence detection takes priority.
	if label, reason, ok := isMediumOpaqueGoType(t); ok {
		return TypeInfo{Kind: "opaque", Label: label, MediumOpacity: reason}
	}
	if ptr, ok := t.(*types.Pointer); ok {
		if label, reason, ok := isMediumOpaqueGoType(ptr.Elem()); ok {
			return TypeInfo{Kind: "opaque", Label: label, MediumOpacity: reason}
		}
	}
	switch typ := t.Underlying().(type) {
	case *types.Basic:
		return basicTypeInfo(typ)
	case *types.Slice:
		elem := goTypeToTypeInfoRec(typ.Elem(), fc, visited)
		return TypeInfo{Kind: "array", Element: &elem}
	case *types.Array:
		elem := goTypeToTypeInfoRec(typ.Elem(), fc, visited)
		return TypeInfo{Kind: "array", Element: &elem}
	case *types.Map:
		return mapTypeInfoRec(typ, fc, visited)
	case *types.Struct:
		return structTypeInfoRec(typ, fc, visited)
	case *types.Pointer:
		inner := goTypeToTypeInfoRec(typ.Elem(), fc, visited)
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
	return mapTypeInfoRec(m, nil, make(map[types.Type]bool))
}

func mapTypeInfoRec(m *types.Map, fc *fileContext, visited map[types.Type]bool) TypeInfo {
	keyType := goTypeToTypeInfoRec(m.Key(), fc, visited)
	valType := goTypeToTypeInfoRec(m.Elem(), fc, visited)
	return TypeInfo{
		Kind: "object",
		Fields: []ObjectField{
			{Name: "_key", Type: keyType},
			{Name: "_value", Type: valType},
		},
	}
}

func structTypeInfo(s *types.Struct) TypeInfo {
	return structTypeInfoRec(s, nil, make(map[types.Type]bool))
}

func structTypeInfoRec(s *types.Struct, fc *fileContext, visited map[types.Type]bool) TypeInfo {
	fields := make([]ObjectField, s.NumFields())
	for i := 0; i < s.NumFields(); i++ {
		f := s.Field(i)
		fields[i] = ObjectField{
			Name: f.Name(),
			Type: goTypeToTypeInfoRec(f.Type(), fc, visited),
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

// extractLoops walks the function body in AST traversal order (matching the
// instrumentor's loop numbering) and returns LoopInfo for any canonical counted
// for-loop whose induction variable can be fully characterized. The nextLoopID
// counter increments for every for or range statement so the IDs stay in sync
// with the instrumentor's numbering even when some loops are not canonical.
func extractLoops(fset *token.FileSet, body *ast.BlockStmt, params map[string]bool) []LoopInfo {
	var loops []LoopInfo
	nextLoopID := 0

	ast.Inspect(body, func(n ast.Node) bool {
		switch stmt := n.(type) {
		case *ast.ForStmt:
			loopID := nextLoopID
			nextLoopID++
			iv := analyzeForStmtInductionVar(fset, stmt, params)
			if iv != nil {
				loops = append(loops, LoopInfo{
					LoopID:       loopID,
					Line:         fset.Position(stmt.Pos()).Line,
					InductionVar: iv,
				})
			}
		case *ast.RangeStmt:
			nextLoopID++
		}
		return true
	})

	return loops
}

// analyzeForStmtInductionVar attempts to extract a canonical induction variable
// from a for-statement of the form:
//
//	for i := init; i op bound; i++ { ... }
//	for i := init; i op bound; i += step { ... }
//
// Returns nil if the loop does not match the canonical pattern or if the
// induction variable is modified inside the loop body.
func analyzeForStmtInductionVar(fset *token.FileSet, stmt *ast.ForStmt, params map[string]bool) *InductionVar {
	// Init must be a short variable declaration with exactly one LHS identifier.
	initAssign, ok := stmt.Init.(*ast.AssignStmt)
	if !ok || initAssign.Tok != token.DEFINE || len(initAssign.Lhs) != 1 {
		return nil
	}
	initIdent, ok := initAssign.Lhs[0].(*ast.Ident)
	if !ok {
		return nil
	}
	varName := initIdent.Name

	// Condition must be a binary comparison where one side is the induction variable.
	condBin, ok := stmt.Cond.(*ast.BinaryExpr)
	if !ok {
		return nil
	}
	var boundExprAST ast.Expr
	var boundOp string
	switch condBin.Op {
	case token.LSS:
		if isIdent(condBin.X, varName) {
			boundOp = "lt"
			boundExprAST = condBin.Y
		} else if isIdent(condBin.Y, varName) {
			boundOp = "gt"
			boundExprAST = condBin.X
		} else {
			return nil
		}
	case token.LEQ:
		if isIdent(condBin.X, varName) {
			boundOp = "le"
			boundExprAST = condBin.Y
		} else if isIdent(condBin.Y, varName) {
			boundOp = "ge"
			boundExprAST = condBin.X
		} else {
			return nil
		}
	case token.GTR:
		if isIdent(condBin.X, varName) {
			boundOp = "gt"
			boundExprAST = condBin.Y
		} else if isIdent(condBin.Y, varName) {
			boundOp = "lt"
			boundExprAST = condBin.X
		} else {
			return nil
		}
	case token.GEQ:
		if isIdent(condBin.X, varName) {
			boundOp = "ge"
			boundExprAST = condBin.Y
		} else if isIdent(condBin.Y, varName) {
			boundOp = "le"
			boundExprAST = condBin.X
		} else {
			return nil
		}
	default:
		return nil
	}

	// Post must be an increment/decrement or compound assignment on the induction variable.
	var stepExpr *SymExpr
	switch post := stmt.Post.(type) {
	case *ast.IncDecStmt:
		if !isIdent(post.X, varName) {
			return nil
		}
		if post.Tok == token.INC {
			stepExpr = &SymExpr{Kind: "const", Type: "int", Value: int64(1), Args: []SymExpr{}}
		} else if post.Tok == token.DEC {
			stepExpr = &SymExpr{Kind: "const", Type: "int", Value: int64(-1), Args: []SymExpr{}}
		} else {
			return nil
		}
	case *ast.AssignStmt:
		if len(post.Lhs) != 1 || !isIdent(post.Lhs[0], varName) {
			return nil
		}
		if len(post.Rhs) != 1 {
			return nil
		}
		rhsSym := buildSymExpr(post.Rhs[0], params)
		if post.Tok == token.ADD_ASSIGN {
			stepExpr = rhsSym
		} else if post.Tok == token.SUB_ASSIGN {
			// Negate the RHS to express subtraction as a negative step.
			stepExpr = &SymExpr{Kind: "un_op", Op: "neg", Operand: rhsSym, Args: []SymExpr{}}
		} else {
			return nil
		}
	default:
		return nil
	}

	// Verify the induction variable is not modified inside the loop body.
	if inductionVarModifiedInBody(stmt.Body, varName) {
		return nil
	}

	// Build symbolic expressions for init and bound using the params context.
	// The init RHS may reference params (e.g. for i := start; ...).
	var initExpr *SymExpr
	if len(initAssign.Rhs) == 1 {
		initExpr = buildSymExpr(initAssign.Rhs[0], params)
	} else {
		initExpr = &SymExpr{Kind: "unknown", Args: []SymExpr{}}
	}
	boundExpr := buildSymExpr(boundExprAST, params)

	return &InductionVar{
		Name:      varName,
		InitExpr:  initExpr,
		StepExpr:  stepExpr,
		BoundExpr: boundExpr,
		BoundOp:   boundOp,
	}
}

// isIdent returns true if expr is an *ast.Ident with the given name.
func isIdent(expr ast.Expr, name string) bool {
	id, ok := expr.(*ast.Ident)
	return ok && id.Name == name
}

// inductionVarModifiedInBody returns true if varName is assigned or
// incremented/decremented anywhere inside body.
func inductionVarModifiedInBody(body *ast.BlockStmt, varName string) bool {
	modified := false
	ast.Inspect(body, func(n ast.Node) bool {
		if modified {
			return false
		}
		switch s := n.(type) {
		case *ast.AssignStmt:
			for _, lhs := range s.Lhs {
				if isIdent(lhs, varName) {
					modified = true
					return false
				}
			}
		case *ast.IncDecStmt:
			if isIdent(s.X, varName) {
				modified = true
				return false
			}
		}
		return true
	})
	return modified
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
		return &SymExpr{Kind: "unknown", Args: []SymExpr{}}
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
		Args:  []SymExpr{},
	}
}

func buildUnOp(expr *ast.UnaryExpr, params map[string]bool) *SymExpr {
	var op string
	switch expr.Op {
	case token.SUB:
		op = "neg"
	case token.XOR:
		op = "bitwise_not"
	case token.NOT:
		op = "not"
	default:
		return &SymExpr{Kind: "unknown", Args: []SymExpr{}}
	}
	operand := buildSymExpr(expr.X, params)
	return &SymExpr{
		Kind:    "un_op",
		Op:      op,
		Operand: operand,
		Args:    []SymExpr{},
	}
}

func identSymExpr(ident *ast.Ident, params map[string]bool) *SymExpr {
	if params[ident.Name] {
		return &SymExpr{
			Kind: "param",
			Name: ident.Name,
			Path: []string{},
			Args: []SymExpr{},
		}
	}
	// Not a param — treat as unknown variable reference
	return &SymExpr{Kind: "unknown", Args: []SymExpr{}}
}

func selectorSymExpr(sel *ast.SelectorExpr, params map[string]bool) *SymExpr {
	// Check if the root is a parameter (e.g., order.Priority)
	name, path := flattenSelector(sel)
	if params[name] {
		return &SymExpr{
			Kind: "param",
			Name: name,
			Path: path,
			Args: []SymExpr{},
		}
	}
	return &SymExpr{Kind: "unknown", Args: []SymExpr{}}
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
			return &SymExpr{Kind: "unknown", Args: []SymExpr{}}
		}
		return &SymExpr{Kind: "const", Type: "int", Value: n, Args: []SymExpr{}}
	case token.FLOAT:
		f, err := strconv.ParseFloat(lit.Value, 64)
		if err != nil {
			return &SymExpr{Kind: "unknown", Args: []SymExpr{}}
		}
		return &SymExpr{Kind: "const", Type: "float", Value: f, Args: []SymExpr{}}
	case token.STRING, token.CHAR:
		// Strip quotes for the value
		val := strings.Trim(lit.Value, "`\"'")
		return &SymExpr{Kind: "const", Type: "str", Value: val, Args: []SymExpr{}}
	default:
		return &SymExpr{Kind: "unknown", Args: []SymExpr{}}
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

	// Always initialize args as an empty slice (even for zero-arg calls)
	// to ensure it serializes as [] not null, preventing Rust deserialization errors.
	args := make([]SymExpr, len(call.Args))
	for i, arg := range call.Args {
		sym := buildSymExpr(arg, params)
		if sym != nil {
			args[i] = *sym
		} else {
			args[i] = SymExpr{Kind: "unknown", Args: []SymExpr{}}
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
		Args:  []SymExpr{},
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
	case token.AND:
		return "bitwise_and"
	case token.OR:
		return "bitwise_or"
	case token.XOR:
		return "bitwise_xor"
	case token.SHL:
		return "shl"
	case token.SHR:
		return "shr"
	case token.AND_NOT:
		return "bit_clear"
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

// --- Literal Extraction ---

// extractLiterals walks a function body, file-level const/var declarations,
// and map key accesses to collect literal constant values as candidate test inputs.
// Results are deduplicated by (type, value) pair.
func extractLiterals(fn *ast.FuncDecl, file *ast.File) []LiteralValue {
	if fn.Body == nil {
		return nil
	}

	seen := make(map[string]bool)
	var results []LiteralValue

	add := func(lit LiteralValue) {
		var key string
		if lit.Pattern != "" {
			key = "regex:" + lit.Pattern
		} else {
			key = fmt.Sprintf("%s:%v", lit.Type, lit.Value)
		}
		if !seen[key] {
			seen[key] = true
			results = append(results, lit)
		}
	}

	ast.Inspect(fn.Body, func(n ast.Node) bool {
		switch node := n.(type) {
		case *ast.BasicLit:
			switch node.Kind {
			case token.INT:
				if v, err := strconv.ParseInt(node.Value, 0, 64); err == nil {
					add(LiteralValue{Type: "int", Value: v})
				}
			case token.FLOAT:
				if v, err := strconv.ParseFloat(node.Value, 64); err == nil {
					add(LiteralValue{Type: "float", Value: v})
				}
			case token.STRING, token.CHAR:
				s, err := strconv.Unquote(node.Value)
				if err != nil {
					s = strings.Trim(node.Value, "`\"'")
				}
				add(LiteralValue{Type: "str", Value: s})
			}
		case *ast.Ident:
			switch node.Name {
			case "true":
				add(LiteralValue{Type: "bool", Value: true})
			case "false":
				add(LiteralValue{Type: "bool", Value: false})
			}
		case *ast.IndexExpr:
			// Extract map bracket-access string keys: m["status"]
			if lit, ok := node.Index.(*ast.BasicLit); ok && (lit.Kind == token.STRING) {
				s, err := strconv.Unquote(lit.Value)
				if err != nil {
					s = strings.Trim(lit.Value, "`\"'")
				}
				add(LiteralValue{Type: "str", Value: s})
			}
		case *ast.CallExpr:
			// Detect regexp.Compile("pattern") and regexp.MustCompile("pattern")
			if sel, ok := node.Fun.(*ast.SelectorExpr); ok {
				if (sel.Sel.Name == "Compile" || sel.Sel.Name == "MustCompile") && len(node.Args) >= 1 {
					if pkgIdent, ok := sel.X.(*ast.Ident); ok && pkgIdent.Name == "regexp" {
						if lit, ok := node.Args[0].(*ast.BasicLit); ok && (lit.Kind == token.STRING) {
							s, err := strconv.Unquote(lit.Value)
							if err != nil {
								s = strings.Trim(lit.Value, "`\"")
							}
							pkey := "regex:" + s
							if !seen[pkey] {
								seen[pkey] = true
								results = append(results, LiteralValue{Type: "regex", Pattern: s})
							}
						}
					}
				}
			}
		}
		return true
	})

	// Extract file-level const and var declarations with literal values
	for _, decl := range file.Decls {
		gd, ok := decl.(*ast.GenDecl)
		if !ok {
			continue
		}
		if gd.Tok != token.CONST && gd.Tok != token.VAR {
			continue
		}
		for _, spec := range gd.Specs {
			vs, ok := spec.(*ast.ValueSpec)
			if !ok {
				continue
			}
			for _, val := range vs.Values {
				switch lit := val.(type) {
				case *ast.BasicLit:
					switch lit.Kind {
					case token.INT:
						if v, err := strconv.ParseInt(lit.Value, 0, 64); err == nil {
							add(LiteralValue{Type: "int", Value: v})
						}
					case token.FLOAT:
						if v, err := strconv.ParseFloat(lit.Value, 64); err == nil {
							add(LiteralValue{Type: "float", Value: v})
						}
					case token.STRING, token.CHAR:
						s, err := strconv.Unquote(lit.Value)
						if err != nil {
							s = strings.Trim(lit.Value, "`\"'")
						}
						add(LiteralValue{Type: "str", Value: s})
					}
				case *ast.UnaryExpr:
					// Handle negative constants: const MinVal = -100
					if lit.Op == token.SUB {
						if bl, ok := lit.X.(*ast.BasicLit); ok {
							switch bl.Kind {
							case token.INT:
								if v, err := strconv.ParseInt(bl.Value, 0, 64); err == nil {
									add(LiteralValue{Type: "int", Value: -v})
								}
							case token.FLOAT:
								if v, err := strconv.ParseFloat(bl.Value, 64); err == nil {
									add(LiteralValue{Type: "float", Value: -v})
								}
							}
						}
					}
				}
			}
		}
	}

	if results == nil {
		return []LiteralValue{}
	}
	return results
}
