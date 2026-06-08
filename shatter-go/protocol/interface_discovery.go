// str-4v9h: Discover interface implementation candidates for function parameters.
//
// When a parameter is typed as an imported interface (e.g. response.Generator),
// this module scans the interface's defining package for parameterless
// constructors that return types implementing the interface. The discovered
// candidates are attached to TargetContext.InterfaceImplsByParam so the planner
// can select them via PlanInterfaceImpls.
package protocol

import (
	"go/ast"
	"go/types"
	"sort"

	"golang.org/x/tools/go/packages"
)

// discoverInterfaceImplCandidates scans fn's parameters for imported interface
// types and discovers parameterless constructors in each interface's defining
// package whose return type implements the interface. Returns a map from
// parameter name to implementation candidates.
//
// The defining package must already be loaded in consumerPkg.Imports (which is
// the case when the consumer package imports the interface — the acceptance
// criterion). Only constructors with zero parameters are included.
func discoverInterfaceImplCandidates(
	consumerPkg *packages.Package,
	fn *ast.FuncDecl,
) map[string][]InterfaceParamCandidate {
	if consumerPkg == nil || consumerPkg.TypesInfo == nil || fn == nil || fn.Type.Params == nil {
		return nil
	}
	return discoverInterfaceImplCandidatesForParams(consumerPkg, fn.Type.Params)
}

func discoverConstructorInterfaceImplCandidates(
	consumerPkg *packages.Package,
	constructors []ConstructorCandidate,
) map[string][]InterfaceParamCandidate {
	if consumerPkg == nil || consumerPkg.TypesInfo == nil || len(constructors) == 0 {
		return nil
	}
	result := make(map[string][]InterfaceParamCandidate)

	for _, constructor := range constructors {
		fn := findFuncDeclByBareName(consumerPkg, constructor.FuncName)
		if fn == nil || fn.Type.Params == nil {
			continue
		}
		for name, candidates := range discoverInterfaceImplCandidatesForParams(consumerPkg, fn.Type.Params) {
			result[name] = candidates
		}
	}

	if len(result) == 0 {
		return nil
	}
	return result
}

func discoverConstructorRuntimeValues(
	consumerPkg *packages.Package,
	constructors []ConstructorCandidate,
) map[string]ConstructorRuntimeValue {
	if consumerPkg == nil || consumerPkg.TypesInfo == nil || len(constructors) == 0 {
		return nil
	}
	result := make(map[string]ConstructorRuntimeValue)

	for _, constructor := range constructors {
		fn := findFuncDeclByBareName(consumerPkg, constructor.FuncName)
		if fn == nil || fn.Type.Params == nil {
			continue
		}
		for _, field := range fn.Type.Params.List {
			value, ok := discoverConstructorRuntimeValueForField(consumerPkg, field)
			if !ok {
				continue
			}
			for _, name := range field.Names {
				result[name.Name] = value
			}
		}
	}

	if len(result) == 0 {
		return nil
	}
	return result
}

func discoverConstructorRuntimeValueForField(
	consumerPkg *packages.Package,
	field *ast.Field,
) (ConstructorRuntimeValue, bool) {
	if consumerPkg == nil || consumerPkg.TypesInfo == nil || field == nil {
		return ConstructorRuntimeValue{}, false
	}
	named, wantsPointer := resolveNamedConcreteType(field.Type, consumerPkg.TypesInfo)
	if named == nil || named.Obj() == nil || named.Obj().Pkg() == nil {
		return ConstructorRuntimeValue{}, false
	}
	if _, isInterface := named.Underlying().(*types.Interface); isInterface {
		return ConstructorRuntimeValue{}, false
	}

	defPkgPath := named.Obj().Pkg().Path()
	if defPkgPath == "" || defPkgPath == consumerPkg.PkgPath {
		return ConstructorRuntimeValue{}, false
	}
	defPkg := consumerPkg.Imports[defPkgPath]
	if defPkg == nil || defPkg.TypesInfo == nil {
		return ConstructorRuntimeValue{}, false
	}

	var candidates []ConstructorCandidate
	for _, constructor := range ScanConstructors(defPkg) {
		if constructor.TargetType != named.Obj().Name() {
			continue
		}
		if constructor.ReturnsPointer != wantsPointer {
			continue
		}
		if len(constructor.Parameters) > 0 {
			continue
		}
		candidates = append(candidates, constructor)
	}
	if len(candidates) == 0 {
		return ConstructorRuntimeValue{}, false
	}
	sort.SliceStable(candidates, func(i, j int) bool {
		return candidates[i].FuncName < candidates[j].FuncName
	})

	pkgAlias := pkgAliasInConsumer(consumerPkg, defPkg.PkgPath)
	if pkgAlias == "" {
		pkgAlias = defPkg.Name
	}
	return ConstructorRuntimeValue{
		Expression: pkgAlias + "." + candidates[0].FuncName + "()",
		Imports:    []string{defPkg.PkgPath},
	}, true
}

func resolveNamedConcreteType(expr ast.Expr, info *types.Info) (*types.Named, bool) {
	if expr == nil || info == nil {
		return nil, false
	}
	tv, ok := info.Types[expr]
	if !ok || tv.Type == nil {
		return nil, false
	}
	typ := tv.Type
	isPointer := false
	if ptr, ok := typ.(*types.Pointer); ok {
		isPointer = true
		typ = ptr.Elem()
	}
	named, ok := typ.(*types.Named)
	if !ok {
		return nil, false
	}
	return named, isPointer
}

func discoverInterfaceImplCandidatesForParams(
	consumerPkg *packages.Package,
	params *ast.FieldList,
) map[string][]InterfaceParamCandidate {
	if consumerPkg == nil || consumerPkg.TypesInfo == nil || params == nil {
		return nil
	}

	result := make(map[string][]InterfaceParamCandidate)

	for _, field := range params.List {
		ifaceType, ifaceNamed := resolveInterfaceType(field.Type, consumerPkg.TypesInfo)
		if ifaceType == nil || ifaceNamed == nil {
			continue
		}

		ifacePkg := ifaceNamed.Obj().Pkg()
		if ifacePkg == nil {
			continue
		}

		// Only process imported interfaces (not same-package).
		if ifacePkg.Path() == consumerPkg.PkgPath {
			continue
		}

		// Look up the defining package in the consumer's imports.
		defPkg, ok := consumerPkg.Imports[ifacePkg.Path()]
		if !ok || defPkg == nil || defPkg.TypesInfo == nil {
			continue
		}

		candidates := findImplConstructors(defPkg, ifaceType, consumerPkg)
		if len(candidates) == 0 {
			continue
		}

		for _, name := range field.Names {
			result[name.Name] = candidates
		}
	}

	if len(result) == 0 {
		return nil
	}
	return result
}

// resolveInterfaceType checks whether expr resolves to a named interface type.
// Returns the underlying *types.Interface and the *types.Named wrapper, or
// (nil, nil) if the expression is not an interface.
func resolveInterfaceType(expr ast.Expr, info *types.Info) (*types.Interface, *types.Named) {
	if info == nil {
		return nil, nil
	}
	tv, ok := info.Types[expr]
	if !ok {
		return nil, nil
	}
	named, ok := tv.Type.(*types.Named)
	if !ok {
		return nil, nil
	}
	iface, ok := named.Underlying().(*types.Interface)
	if !ok {
		return nil, nil
	}
	return iface, named
}

// findImplConstructors scans the defining package for parameterless
// constructors whose return type implements the interface.
func findImplConstructors(
	defPkg *packages.Package,
	ifaceType *types.Interface,
	consumerPkg *packages.Package,
) []InterfaceParamCandidate {
	allConstructors := ScanConstructors(defPkg)

	// Derive the package alias used in the consumer's imports so expressions
	// are qualified correctly (e.g. "response.NewFakerGenerator()").
	pkgAlias := pkgAliasInConsumer(consumerPkg, defPkg.PkgPath)
	if pkgAlias == "" {
		pkgAlias = defPkg.Name
	}

	// Group parameterless constructors by return type, then check if the
	// return type implements the interface.
	type implEntry struct {
		typeName     string
		constructors []ConstructorCandidate
	}
	byType := make(map[string]*implEntry)

	for _, ctor := range allConstructors {
		// Only parameterless constructors per acceptance criteria.
		if len(ctor.Parameters) > 0 {
			continue
		}

		// Check if the constructor's return type implements the interface.
		returnType := lookupNamedType(defPkg, ctor.TargetType)
		if returnType == nil {
			continue
		}

		// types.Implements checks if the type (or *type) satisfies the interface.
		ptrType := types.NewPointer(returnType)
		if !types.Implements(returnType, ifaceType) && !types.Implements(ptrType, ifaceType) {
			continue
		}

		entry, ok := byType[ctor.TargetType]
		if !ok {
			entry = &implEntry{typeName: ctor.TargetType}
			byType[ctor.TargetType] = entry
		}

		// Qualify the constructor name with the package alias.
		qualifiedCtor := ctor
		qualifiedCtor.FuncName = pkgAlias + "." + ctor.FuncName
		entry.constructors = append(entry.constructors, qualifiedCtor)
	}

	if len(byType) == 0 {
		return nil
	}

	candidates := make([]InterfaceParamCandidate, 0, len(byType))
	for _, entry := range byType {
		candidates = append(candidates, InterfaceParamCandidate{
			TypeName:     entry.typeName,
			SamePackage:  false,
			Constructors: entry.constructors,
			ImportPath:   defPkg.PkgPath,
		})
	}
	return candidates
}

// lookupNamedType finds a named type by bare name in a loaded package.
func lookupNamedType(pkg *packages.Package, name string) types.Type {
	if pkg.Types == nil {
		return nil
	}
	obj := pkg.Types.Scope().Lookup(name)
	if obj == nil {
		return nil
	}
	tn, ok := obj.(*types.TypeName)
	if !ok {
		return nil
	}
	return tn.Type()
}

// pkgAliasInConsumer returns the import alias used for importPath in the
// consumer package's source files. Returns "" if not found (caller should
// fall back to the package's default name).
func pkgAliasInConsumer(consumerPkg *packages.Package, importPath string) string {
	if consumerPkg == nil {
		return ""
	}
	for _, file := range consumerPkg.Syntax {
		if file == nil {
			continue
		}
		for _, imp := range file.Imports {
			path := imp.Path.Value
			// Strip quotes.
			if len(path) >= 2 {
				path = path[1 : len(path)-1]
			}
			if path != importPath {
				continue
			}
			if imp.Name != nil && imp.Name.Name != "_" && imp.Name.Name != "." {
				return imp.Name.Name
			}
			// No explicit alias — use the package's default name.
			if consumerPkg.Types != nil {
				for _, p := range consumerPkg.Types.Imports() {
					if p.Path() == importPath {
						return p.Name()
					}
				}
			}
			return ""
		}
	}
	return ""
}
