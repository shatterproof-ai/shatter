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

	"github.com/shatter-dev/shatter/shatter-go/config"
	"github.com/shatter-dev/shatter/shatter-go/runtimeval"
)

// WrapperParam describes one parameter of a wrapper target, including the
// Go type name required for code generation.
//
// IsVariadic is true only for the final positional parameter of a function
// declared with `...T`. The GoType for a variadic parameter is the slice
// form (`[]T`); the call site must expand it with `args...` so the wrapper
// passes through to the target's variadic call shape (str-jeen.48).
type WrapperParam struct {
	Name                                string
	GoType                              string // concrete Go type string, e.g. "int", "*Counter", "string"
	IsVariadic                          bool
	NeedsMapInputNormalization          bool
	NeedsTimeInputNormalization         bool
	NeedsFuncInputNormalization         bool
	NeedsRuntimeValueInputNormalization bool
	// RuntimeValueExpr, when non-empty, carries a Go-source expression
	// (e.g. `context.Background()`, `httptest.NewRecorder()`) that the
	// wrapper substitutes for the parameter's value instead of decoding
	// from `inputs[i]` (str-gxjs.1). The expression comes from the
	// planner's runtime-value registry keyed by GoType; the necessary
	// import paths are added to the owning WrapperTarget.Imports list.
	// Empty for parameters that follow the JSON-input path.
	RuntimeValueExpr string
}

// TypeParamInfo describes one generic type parameter declared by a wrapper target.
type TypeParamInfo struct {
	Name       string
	Constraint string
}

// WrapperTarget is an enriched description of a discovered invocation target
// with Go-level type information for code generation.
type WrapperTarget struct {
	ID                string // stable target ID, e.g. "example.com/pkg:Add"
	SymbolName        string // bare function or method name
	Kind              TargetKind
	ReceiverType      string // bare type name (without *) for method targets
	IsPointerRecv     bool   // true for (*T).Method receivers
	ReceiverMapFields []ReceiverMapField
	Parameters        []WrapperParam
	TypeParams        []TypeParamInfo
	HasResult         bool
	ResultGoType      string // Go type string for the first return value
	ResultGoTypes     []string
	ResultCount       int // total number of return values (0 when HasResult is false)
	// Imports lists the import paths required by qualified type names that the
	// generated wrapper source actually references. Today that means parameter
	// types such as `context.Context`, `*pgx.Conn`, or `gqlerror.Error`; result
	// types are tracked in ResultGoType for metadata, but the generated source
	// does not name them and must not import result-only packages.
	// Result type names are tracked for call-shape decisions such as
	// conventional trailing error propagation, but result-only package imports
	// are still intentionally omitted. Cross-ref: str-jeen.33 and str-iylc.
	Imports []string
}

// ReceiverMapField describes a receiver map field the wrapper can initialize
// from inside the target package.
type ReceiverMapField struct {
	Name   string
	GoType string
}

const (
	// WrapperKindZeroValue selects zero-value receiver construction.
	WrapperKindZeroValue = "zero_value"
	// WrapperKindInitializedMaps selects receiver construction that allocates
	// map fields before invoking the target method.
	WrapperKindInitializedMaps = "initialized_maps"
	// WrapperKindConstructorPrefix is prepended to a constructor function
	// name to form the ReceiverKind string.
	WrapperKindConstructorPrefix = "constructor:"
)

// generatorVersion is bumped whenever the wrapper code-generation logic
// changes in a way that produces materially different output for the same
// inputs (new code paths, changed deserialization templates, etc.).
// Including it in DiscoveryHash ensures that stale cached wrappers from a
// previous generator revision are never reused. str-5ac4.
const generatorVersion = "gen-v12"

// DiscoveryHash returns a 16-character hex prefix of the SHA-256 over the
// full target signatures (parameters, results, receiver shape, imports,
// type params), constructor metadata (including parameter types), and a
// generator version constant. The hash is fully determined by the discovery
// results plus the generator revision, so the wrapper filename is stable
// for the same inputs and changes when any code-generation-relevant field
// differs — including parameter type changes, result arity changes,
// runtime-value bindings, and wrapper-generator code changes (str-5ac4).
func DiscoveryHash(targets []WrapperTarget, constructors []ConstructorCandidate) string {
	ids := make([]string, len(targets))
	for i, t := range targets {
		ids[i] = targetSignature(t)
	}
	sort.Strings(ids)

	ctors := make([]string, len(constructors))
	for i, c := range constructors {
		ctors[i] = constructorSignature(c)
	}
	sort.Strings(ctors)

	payload := generatorVersion + "\n" + strings.Join(ids, "\n") + "\n---\n" + strings.Join(ctors, "\n")
	sum := sha256.Sum256([]byte(payload))
	return hex.EncodeToString(sum[:])[:16]
}

// targetSignature returns a deterministic string encoding every field of a
// WrapperTarget that influences the generated wrapper source. Any change
// to parameter types, result shape, receiver kind, imports, or
// runtime-value bindings produces a different signature, invalidating the
// cached wrapper. str-5ac4.
func targetSignature(t WrapperTarget) string {
	var b strings.Builder
	b.WriteString(t.ID)
	b.WriteByte(':')
	b.WriteString(string(t.Kind))
	b.WriteByte(':')
	b.WriteString(t.ReceiverType)
	b.WriteByte(':')
	if t.IsPointerRecv {
		b.WriteByte('1')
	} else {
		b.WriteByte('0')
	}
	b.WriteByte(':')
	b.WriteString(typeParamSignature(t.TypeParams))
	b.WriteByte(':')
	b.WriteString(strings.Join(sortedStrings(t.Imports), ","))
	b.WriteByte(':')
	for i, f := range t.ReceiverMapFields {
		if i > 0 {
			b.WriteByte(';')
		}
		b.WriteString(f.Name)
		b.WriteByte('/')
		b.WriteString(f.GoType)
	}
	b.WriteByte(':')
	// Parameter signatures: type, variadic flag, and runtime-value expression.
	for pi, p := range t.Parameters {
		if pi > 0 {
			b.WriteByte(';')
		}
		b.WriteString(p.Name)
		b.WriteByte('/')
		b.WriteString(p.GoType)
		b.WriteByte('/')
		if p.IsVariadic {
			b.WriteByte('v')
		}
		b.WriteByte('/')
		if p.NeedsMapInputNormalization {
			b.WriteByte('m')
		}
		b.WriteByte('/')
		if p.NeedsTimeInputNormalization {
			b.WriteByte('t')
		}
		b.WriteByte('/')
		if p.NeedsFuncInputNormalization {
			b.WriteByte('f')
		}
		b.WriteByte('/')
		if p.NeedsRuntimeValueInputNormalization {
			b.WriteByte('r')
		}
		b.WriteByte('/')
		b.WriteString(p.RuntimeValueExpr)
	}
	b.WriteByte(':')
	// Result signature.
	if t.HasResult {
		b.WriteByte('1')
	} else {
		b.WriteByte('0')
	}
	b.WriteByte('/')
	b.WriteString(t.ResultGoType)
	b.WriteByte('/')
	fmt.Fprintf(&b, "%d", t.ResultCount)
	b.WriteByte('/')
	b.WriteString(strings.Join(t.ResultGoTypes, ","))
	return b.String()
}

// constructorSignature returns a deterministic string encoding every field
// of a ConstructorCandidate that influences the generated wrapper source,
// including parameter types (str-5ac4).
func constructorSignature(c ConstructorCandidate) string {
	hasParams := "0"
	if c.HasParams {
		hasParams = "1"
	}
	returnsPtr := "0"
	if c.ReturnsPointer {
		returnsPtr = "1"
	}
	returnsErr := "0"
	if c.ReturnsError {
		returnsErr = "1"
	}
	returnsIface := "0"
	if c.ReturnsInterface {
		returnsIface = "1"
	}
	sig := c.FuncName + ":" + c.TargetType + ":" + hasParams + ":" + returnsPtr + ":" + returnsErr + ":" + returnsIface
	// str-5ac4: include actual parameter types so that constructor
	// signature changes invalidate the cached wrapper.
	if len(c.Parameters) > 0 {
		paramParts := make([]string, len(c.Parameters))
		for i, p := range c.Parameters {
			paramParts[i] = p.Name + "/" + p.GoType + "/" + constructorParamRuntimeValueExpr(p)
		}
		sig += ":" + strings.Join(paramParts, ";")
	}
	return sig
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
	for i := range sortedCtors {
		sortedCtors[i].Parameters = append([]ConstructorParam(nil), sortedCtors[i].Parameters...)
	}
	applyRuntimeValueBindingsToConstructors(sortedCtors)
	sort.Slice(sortedCtors, func(i, j int) bool { return sortedCtors[i].FuncName < sortedCtors[j].FuncName })

	// Index constructors by target type for receiver-kind enumeration.
	// Skip constructors whose real signature takes unsatisfiable parameters
	// (str-qo1.14). Parameterized constructors with populated Parameters
	// (str-9b1q) are allowed — the wrapper deserializes their values from
	// the input prefix before method arguments.
	ctorsByType := make(map[string][]ConstructorCandidate)
	for _, c := range sortedCtors {
		if c.HasParams && len(c.Parameters) == 0 {
			// HasParams but no Parameters = unsatisfiable (legacy filter).
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
	// str-jeen.73: collect extra imports first so we can determine whether
	// "strings" is needed from either generic-target generated code OR from
	// parameter types that reference the strings package (e.g. io.Reader whose
	// runtimeval candidate uses strings.NewReader, or strings.Builder params).
	// Previously "strings" was hard-excluded from collectExtraImports as a
	// "core import" and only added conditionally for generic targets, silently
	// dropping it when non-generic targets needed it.
	extraImports := collectExtraImports(sorted, sortedCtors)
	needsStrings := hasGenericTargets(sorted)
	filteredExtra := make([]string, 0, len(extraImports))
	for _, imp := range extraImports {
		if imp == "strings" {
			needsStrings = true
		} else {
			filteredExtra = append(filteredExtra, imp)
		}
	}
	needsTimeNormalizer := wrapperNeedsTimeInputNormalizer(sorted) || constructorsNeedTimeInputNormalizer(sortedCtors)
	if needsTimeNormalizer {
		filteredExtra = appendStringIfMissing(filteredExtra, "time")
		sort.Strings(filteredExtra)
	}
	// str-jn9r0: bare builtin `error` params decode via errors.New in
	// writeErrorParamDeserialization, so thread the "errors" import when any
	// target or constructor takes an error param.
	if wrapperNeedsErrorImport(sorted, sortedCtors) {
		filteredExtra = appendStringIfMissing(filteredExtra, "errors")
		sort.Strings(filteredExtra)
	}
	needsMapNormalizer := wrapperNeedsMapInputNormalizer(sorted) || constructorsNeedMapInputNormalizer(sortedCtors)
	needsFuncNormalizer := wrapperNeedsFuncInputNormalizer(sorted)
	needsRuntimeValueNormalizer := wrapperNeedsRuntimeValueInputNormalizer(sorted)
	if needsFuncNormalizer || needsRuntimeValueNormalizer {
		filteredExtra = appendStringIfMissing(filteredExtra, "reflect")
		sort.Strings(filteredExtra)
	}
	if needsMapNormalizer || needsTimeNormalizer || needsFuncNormalizer || needsRuntimeValueNormalizer {
		needsStrings = true
	}
	if needsStrings {
		b.WriteString("\t\"strings\"\n")
	}
	// str-jeen.33: union the per-target Imports lists and emit one entry per
	// distinct import path. Without this, qualified parameter or return types
	// like context.Context, *pgx.Conn, slog.Logger would leave the generated
	// wrapper file referencing undefined package short names.
	for _, importPath := range filteredExtra {
		fmt.Fprintf(&b, "\t%q\n", importPath)
	}
	b.WriteString(")\n\n")

	if wrapperUsesSyntheticHTTPTransport(sorted, sortedCtors) {
		writeSyntheticHTTPClientHelper(&b)
	}

	b.WriteString("// PlanDescriptor selects one invocation strategy for one ShatterInvoke call.\n")
	b.WriteString("type PlanDescriptor struct {\n")
	b.WriteString("\tTargetID     string `json:\"target_id\"`\n")
	b.WriteString("\tReceiverKind string `json:\"receiver_kind\"`\n")
	b.WriteString("\tGenericTypeArgs []string `json:\"generic_type_args,omitempty\"`\n")
	b.WriteString("}\n\n")
	if needsMapNormalizer {
		writeMapInputNormalizer(&b)
	}
	if needsTimeNormalizer {
		writeTimeInputNormalizer(&b)
	}
	if needsFuncNormalizer {
		writeFuncInputNormalizer(&b)
	}
	if needsRuntimeValueNormalizer {
		writeRuntimeValueInputNormalizer(&b)
	}

	// str-jeen.77: use _shatterInputs instead of inputs so that target
	// functions whose parameters are named "inputs" do not shadow the outer
	// slice. Pre-fix, a target func Resolve(inputs []ResolveInput) caused
	// "var inputs []ResolveInput" inside the switch case to shadow the outer
	// "inputs []json.RawMessage", making inputs[i] refer to a struct value
	// instead of json.RawMessage and producing a "cannot use inputs[N]
	// (variable of struct type ResolveInput) as []byte value" compile error.
	b.WriteString("// ShatterInvoke executes the strategy in d against inputs and returns the result.\n")
	b.WriteString("func ShatterInvoke(d PlanDescriptor, _shatterInputs []json.RawMessage) (any, error) {\n")
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

	if len(t.ReceiverMapFields) > 0 {
		fmt.Fprintf(b, "\t\tcase %q:\n", WrapperKindInitializedMaps)
		if t.IsPointerRecv {
			fmt.Fprintf(b, "\t\t\t_recvVal := %s{\n", t.ReceiverType)
			writeReceiverMapFieldInitializers(b, t.ReceiverMapFields, "\t\t\t\t")
			b.WriteString("\t\t\t}\n")
			b.WriteString("\t\t\t_recv := &_recvVal\n")
		} else {
			fmt.Fprintf(b, "\t\t\t_recv := %s{\n", t.ReceiverType)
			writeReceiverMapFieldInitializers(b, t.ReceiverMapFields, "\t\t\t\t")
			b.WriteString("\t\t\t}\n")
		}
		writeParamDeserialization(b, t.Parameters, "\t\t\t")
		writeCall(b, t, "_recv", nil, "\t\t\t")
	}

	if ctors, ok := ctorsByType[t.ReceiverType]; ok {
		for _, c := range ctors {
			recvKind := WrapperKindConstructorPrefix + c.FuncName
			fmt.Fprintf(b, "\t\tcase %q:\n", recvKind)
			writeConstructorParamDeserialization(b, c.Parameters, "\t\t\t")
			ctorArgs := constructorInputArgExpr(c)
			if c.ReturnsInterface {
				writeInterfaceConstructorReceiver(b, t, c, ctorArgs)
				writeParamDeserializationAtOffset(b, t.Parameters, "\t\t\t", constructorInputSlotCount(c.Parameters))
				writeCall(b, t, "_recv", nil, "\t\t\t")
				continue
			}
			// str-jeen.49: choose the call shape from the cross product
			// of (target receiver kind) × (constructor return kind).
			//
			// str-jeen.78: when the constructor returns (T, error) or
			// (*T, error), use the two-assignment form to avoid assignment
			// mismatch.
			switch {
			case t.IsPointerRecv && c.ReturnsPointer && c.ReturnsError:
				fmt.Fprintf(b, "\t\t\t_recv, _constructorErr := %s(%s)\n", c.FuncName, ctorArgs)
				writeConstructorErrorGuard(b, c.FuncName, "\t\t\t")
				writeConstructorPointerGuard(b, c.FuncName, "_recv", "\t\t\t")
			case t.IsPointerRecv && c.ReturnsPointer:
				fmt.Fprintf(b, "\t\t\t_recv := %s(%s)\n", c.FuncName, ctorArgs)
				writeConstructorPointerGuard(b, c.FuncName, "_recv", "\t\t\t")
			case t.IsPointerRecv && !c.ReturnsPointer && c.ReturnsError:
				fmt.Fprintf(b, "\t\t\t_recvVal, _constructorErr := %s(%s)\n", c.FuncName, ctorArgs)
				writeConstructorErrorGuard(b, c.FuncName, "\t\t\t")
				b.WriteString("\t\t\t_recv := &_recvVal\n")
			case t.IsPointerRecv && !c.ReturnsPointer:
				fmt.Fprintf(b, "\t\t\t_recvVal := %s(%s)\n", c.FuncName, ctorArgs)
				b.WriteString("\t\t\t_recv := &_recvVal\n")
			case !t.IsPointerRecv && c.ReturnsPointer && c.ReturnsError:
				fmt.Fprintf(b, "\t\t\t_recvPtr, _constructorErr := %s(%s)\n", c.FuncName, ctorArgs)
				writeConstructorErrorGuard(b, c.FuncName, "\t\t\t")
				writeConstructorPointerGuard(b, c.FuncName, "_recvPtr", "\t\t\t")
				b.WriteString("\t\t\t_recv := *_recvPtr\n")
			case !t.IsPointerRecv && c.ReturnsPointer:
				fmt.Fprintf(b, "\t\t\t_recvPtr := %s(%s)\n", c.FuncName, ctorArgs)
				writeConstructorPointerGuard(b, c.FuncName, "_recvPtr", "\t\t\t")
				b.WriteString("\t\t\t_recv := *_recvPtr\n")
			case c.ReturnsError: // !t.IsPointerRecv && !c.ReturnsPointer
				fmt.Fprintf(b, "\t\t\t_recv, _constructorErr := %s(%s)\n", c.FuncName, ctorArgs)
				writeConstructorErrorGuard(b, c.FuncName, "\t\t\t")
			default: // !t.IsPointerRecv && !c.ReturnsPointer
				fmt.Fprintf(b, "\t\t\t_recv := %s(%s)\n", c.FuncName, ctorArgs)
			}
			writeParamDeserializationAtOffset(b, t.Parameters, "\t\t\t", constructorInputSlotCount(c.Parameters))
			writeCall(b, t, "_recv", nil, "\t\t\t")
		}
	}

	b.WriteString("\t\t}\n")
	fmt.Fprintf(b, "\t\treturn nil, fmt.Errorf(\"shatter: unknown receiver kind for %s: %%s\", d.ReceiverKind)\n", t.ID)
}

func writeConstructorErrorGuard(b *strings.Builder, funcName, indent string) {
	fmt.Fprintf(b, "%sif _constructorErr != nil {\n", indent)
	fmt.Fprintf(b, "%s\treturn nil, fmt.Errorf(\"shatter: receiver constructor %s failed: %%w\", _constructorErr)\n", indent, funcName)
	fmt.Fprintf(b, "%s}\n", indent)
}

func writeConstructorPointerGuard(b *strings.Builder, funcName, ptrExpr, indent string) {
	fmt.Fprintf(b, "%sif %s == nil {\n", indent, ptrExpr)
	fmt.Fprintf(b, "%s\treturn nil, fmt.Errorf(\"shatter: receiver constructor %s returned nil receiver\")\n", indent, funcName)
	fmt.Fprintf(b, "%s}\n", indent)
}

func writeInterfaceConstructorReceiver(b *strings.Builder, t WrapperTarget, c ConstructorCandidate, ctorArgs string) {
	if c.ReturnsError {
		fmt.Fprintf(b, "\t\t\t_recvIface, _constructorErr := %s(%s)\n", c.FuncName, ctorArgs)
		writeConstructorErrorGuard(b, c.FuncName, "\t\t\t")
	} else {
		fmt.Fprintf(b, "\t\t\t_recvIface := %s(%s)\n", c.FuncName, ctorArgs)
	}

	switch {
	case t.IsPointerRecv && c.ReturnsPointer:
		fmt.Fprintf(b, "\t\t\t_recv, _constructorOK := _recvIface.(*%s)\n", t.ReceiverType)
		writeConstructorTypeAssertionGuard(b, c.FuncName, "_recvIface", "*"+t.ReceiverType, "\t\t\t")
		writeConstructorPointerGuard(b, c.FuncName, "_recv", "\t\t\t")
	case t.IsPointerRecv && !c.ReturnsPointer:
		fmt.Fprintf(b, "\t\t\t_recvVal, _constructorOK := _recvIface.(%s)\n", t.ReceiverType)
		writeConstructorTypeAssertionGuard(b, c.FuncName, "_recvIface", t.ReceiverType, "\t\t\t")
		b.WriteString("\t\t\t_recv := &_recvVal\n")
	case !t.IsPointerRecv && c.ReturnsPointer:
		fmt.Fprintf(b, "\t\t\t_recvPtr, _constructorOK := _recvIface.(*%s)\n", t.ReceiverType)
		writeConstructorTypeAssertionGuard(b, c.FuncName, "_recvIface", "*"+t.ReceiverType, "\t\t\t")
		writeConstructorPointerGuard(b, c.FuncName, "_recvPtr", "\t\t\t")
		b.WriteString("\t\t\t_recv := *_recvPtr\n")
	default:
		fmt.Fprintf(b, "\t\t\t_recv, _constructorOK := _recvIface.(%s)\n", t.ReceiverType)
		writeConstructorTypeAssertionGuard(b, c.FuncName, "_recvIface", t.ReceiverType, "\t\t\t")
	}
}

func writeConstructorTypeAssertionGuard(b *strings.Builder, funcName, valueExpr, wantType, indent string) {
	fmt.Fprintf(b, "%sif !_constructorOK {\n", indent)
	fmt.Fprintf(b, "%s\treturn nil, fmt.Errorf(\"shatter: receiver constructor %s returned %%T, want %s\", %s)\n", indent, funcName, wantType, valueExpr)
	fmt.Fprintf(b, "%s}\n", indent)
}

func writeReceiverMapFieldInitializers(b *strings.Builder, fields []ReceiverMapField, indent string) {
	for _, f := range fields {
		fmt.Fprintf(b, "%s%s: %s{},\n", indent, f.Name, f.GoType)
	}
}

func writeParamDeserialization(b *strings.Builder, params []WrapperParam, indent string) {
	writeParamDeserializationAtOffset(b, params, indent, 0)
}

func writeParamDeserializationAtOffset(b *strings.Builder, params []WrapperParam, indent string, inputOffset int) {
	for i, p := range params {
		writeParamDeserializationAtInputIndex(b, p, i+inputOffset, indent)
	}
}

func writeParamDeserializationAtInputIndex(b *strings.Builder, p WrapperParam, inputIndex int, indent string) {
	if p.RuntimeValueExpr != "" {
		// str-gxjs.1: parameter is satisfied by the planner's
		// runtime-value registry. Emit a direct Go expression
		// (e.g. context.Background()) at the param-init site so
		// the wrapper does not need a JSON input for this slot.
		// The expression must produce a value assignable to GoType
		// — the planner registry enforces that contract.
		fmt.Fprintf(b, "%svar %s %s = %s\n", indent, p.Name, p.GoType, p.RuntimeValueExpr)
		return
	}
	if cand, ok := runtimeval.LookupSymbolic(strings.TrimSpace(p.GoType)); ok {
		writeSymbolicParamDeserialization(b, p.Name, cand, inputIndex, indent)
		return
	}
	if p.GoType == "time.Duration" {
		writeDurationParamDeserialization(b, p.Name, inputIndex, indent)
		return
	}
	if p.GoType == "error" {
		writeErrorParamDeserialization(b, p.Name, inputIndex, indent)
		return
	}
	fmt.Fprintf(b, "%svar %s %s\n", indent, p.Name, p.GoType)
	fmt.Fprintf(b, "%sif %d < len(_shatterInputs) {\n", indent, inputIndex)
	inputExpr := fmt.Sprintf("_shatterInputs[%d]", inputIndex)
	if wrapperParamNeedsMapInputNormalizer(p) {
		inputVar := fmt.Sprintf("_shatterMapInput%d", inputIndex)
		fmt.Fprintf(b, "%s\t%s := shatterNormalizeMapInput(_shatterInputs[%d])\n", indent, inputVar, inputIndex)
		inputExpr = inputVar
	}
	if wrapperParamNeedsTimeInputNormalizer(p) {
		inputVar := fmt.Sprintf("_shatterTimeInput%d", inputIndex)
		fmt.Fprintf(b, "%s\t%s := shatterNormalizeTimeInput(%s)\n", indent, inputVar, inputExpr)
		inputExpr = inputVar
	}
	if wrapperParamNeedsFuncInputNormalizer(p) {
		inputVar := fmt.Sprintf("_shatterFuncInput%d", inputIndex)
		fmt.Fprintf(b, "%s\t%s := shatterNormalizeFuncInput(%s, %s)\n", indent, inputVar, inputExpr, p.Name)
		inputExpr = inputVar
	}
	if wrapperParamNeedsRuntimeValueInputNormalizer(p) {
		inputVar := fmt.Sprintf("_shatterRuntimeValueInput%d", inputIndex)
		fmt.Fprintf(b, "%s\t%s := shatterNormalizeRuntimeValueInput(%s, %s)\n", indent, inputVar, inputExpr, p.Name)
		inputExpr = inputVar
	}
	fmt.Fprintf(b, "%s\tif _e := json.Unmarshal(%s, &%s); _e != nil {\n", indent, inputExpr, p.Name)
	fmt.Fprintf(b, "%s\t\treturn nil, fmt.Errorf(\"param %s: %%w\", _e)\n", indent, p.Name)
	fmt.Fprintf(b, "%s\t}\n", indent)
	fmt.Fprintf(b, "%s}\n", indent)
}

// isSymbolicParam reports whether goType is a symbolic-construction parameter
// (str-ijtww) — one built from a symbolic input slot rather than bound to a
// fixed runtime-value expression. The symbolic type list is single-sourced in
// the runtimeval registry so this stays consistent with the analyzer's slot
// allocation and the planner's body-seed handling. The canonical entry is
// `*http.Request` (str-e41w). The check is intentionally narrow to direct
// params: a symbolic type used as a constructor argument or struct field still
// uses the runtimeval registry's fixed expression (per-input variation there is
// out of scope and routed through different machinery).
func isSymbolicParam(goType string) bool {
	return runtimeval.IsSymbolic(strings.TrimSpace(goType))
}

// writeSymbolicParamDeserialization emits a parameter value whose body is read
// from the param's symbolic input slot (str-e41w / str-ijtww). The body decode
// scaffolding is uniform across symbolic types; the per-type construction (the
// httptest.NewRequest call and header stubs for *http.Request) comes from the
// registry candidate's Construction template, so a new symbolic type is a
// one-line registry addition rather than a new wrapper code path. Each
// Construction entry is a fmt format string whose %[1]s is the parameter
// variable name and %[2]s is the body-input variable.
func writeSymbolicParamDeserialization(b *strings.Builder, name string, cand runtimeval.SymbolicCandidate, inputIndex int, indent string) {
	bodyVar := fmt.Sprintf("_shatterReqBody%d", inputIndex)
	fmt.Fprintf(b, "%svar %s string\n", indent, bodyVar)
	fmt.Fprintf(b, "%sif %d < len(_shatterInputs) {\n", indent, inputIndex)
	fmt.Fprintf(b, "%s\tif _e := json.Unmarshal(_shatterInputs[%d], &%s); _e != nil {\n", indent, inputIndex, bodyVar)
	fmt.Fprintf(b, "%s\t\treturn nil, fmt.Errorf(\"param %s body: %%w\", _e)\n", indent, name)
	fmt.Fprintf(b, "%s\t}\n", indent)
	fmt.Fprintf(b, "%s}\n", indent)
	for _, stmt := range cand.Construction {
		fmt.Fprintf(b, "%s%s\n", indent, fmt.Sprintf(stmt, name, bodyVar))
	}
}

func wrapperNeedsMapInputNormalizer(targets []WrapperTarget) bool {
	for _, t := range targets {
		for _, p := range t.Parameters {
			if wrapperParamNeedsMapInputNormalizer(p) {
				return true
			}
		}
	}
	return false
}

func wrapperParamNeedsMapInputNormalizer(p WrapperParam) bool {
	return p.RuntimeValueExpr == "" && (p.NeedsMapInputNormalization || strings.Contains(p.GoType, "map["))
}

func constructorsNeedMapInputNormalizer(constructors []ConstructorCandidate) bool {
	for _, c := range constructors {
		for _, p := range c.Parameters {
			if strings.Contains(p.GoType, "map[") {
				return true
			}
		}
	}
	return false
}

// wrapperNeedsErrorImport reports whether any target or constructor parameter
// is a bare builtin `error` that the wrapper will actually decode via
// errors.New in writeErrorParamDeserialization (str-jn9r0), and therefore
// require the "errors" import. Threaded through the import block the same way
// "time" is for Duration params. The predicate must match the emission path
// exactly: params satisfied by a runtime-value expression emit a direct Go
// expression instead of the errors.New block (writeParamDeserialization*
// short-circuits on RuntimeValueExpr), so those must NOT thread the import —
// otherwise the generated wrapper carries an unused "errors" import and fails
// `go build`. Targets short-circuit on p.RuntimeValueExpr; constructor params
// resolve through constructorParamRuntimeValueExpr, which also consults the
// runtimeval registry, so use that predicate for the ctor side.
func wrapperNeedsErrorImport(targets []WrapperTarget, constructors []ConstructorCandidate) bool {
	for _, t := range targets {
		for _, p := range t.Parameters {
			if p.RuntimeValueExpr == "" && p.GoType == "error" {
				return true
			}
		}
	}
	for _, c := range constructors {
		for _, p := range c.Parameters {
			if p.GoType == "error" && constructorParamRuntimeValueExpr(p) == "" {
				return true
			}
		}
	}
	return false
}

func wrapperNeedsTimeInputNormalizer(targets []WrapperTarget) bool {
	for _, t := range targets {
		for _, p := range t.Parameters {
			if wrapperParamNeedsTimeInputNormalizer(p) {
				return true
			}
		}
	}
	return false
}

func constructorsNeedTimeInputNormalizer(constructors []ConstructorCandidate) bool {
	for _, c := range constructors {
		for _, p := range c.Parameters {
			if strings.TrimSpace(p.GoType) == "time.Duration" || strings.HasSuffix(strings.TrimSpace(p.GoType), ".Duration") {
				return true
			}
		}
	}
	return false
}

func wrapperParamNeedsTimeInputNormalizer(p WrapperParam) bool {
	return p.RuntimeValueExpr == "" && (p.NeedsTimeInputNormalization || strings.Contains(p.GoType, "time.Time"))
}

func wrapperNeedsFuncInputNormalizer(targets []WrapperTarget) bool {
	for _, t := range targets {
		for _, p := range t.Parameters {
			if wrapperParamNeedsFuncInputNormalizer(p) {
				return true
			}
		}
	}
	return false
}

func wrapperParamNeedsFuncInputNormalizer(p WrapperParam) bool {
	return p.RuntimeValueExpr == "" && p.NeedsFuncInputNormalization
}

func wrapperNeedsRuntimeValueInputNormalizer(targets []WrapperTarget) bool {
	for _, t := range targets {
		for _, p := range t.Parameters {
			if wrapperParamNeedsRuntimeValueInputNormalizer(p) {
				return true
			}
		}
	}
	return false
}

func wrapperParamNeedsRuntimeValueInputNormalizer(p WrapperParam) bool {
	return p.RuntimeValueExpr == "" && p.NeedsRuntimeValueInputNormalization
}

func writeConstructorParamDeserialization(b *strings.Builder, params []ConstructorParam, indent string) {
	if len(params) == 0 {
		return
	}
	inputIndex := 0
	for i, p := range params {
		wrapped := WrapperParam{
			Name:             fmt.Sprintf("_shatterCtorArg%d", i),
			GoType:           p.GoType,
			RuntimeValueExpr: constructorParamRuntimeValueExpr(p),
		}
		writeParamDeserializationAtInputIndex(b, wrapped, inputIndex, indent)
		if wrapped.RuntimeValueExpr == "" {
			inputIndex++
		}
	}
}

func constructorInputSlotCount(params []ConstructorParam) int {
	count := 0
	for _, p := range params {
		if constructorParamRuntimeValueExpr(p) == "" {
			count++
		}
	}
	return count
}

func constructorParamRuntimeValueExpr(p ConstructorParam) string {
	if p.RuntimeValueExpr != "" {
		return p.RuntimeValueExpr
	}
	candidates := runtimeval.Lookup(p.GoType)
	if len(candidates) == 0 {
		return ""
	}
	return candidates[0].Expression
}

func applyRuntimeValueBindingsToConstructors(constructors []ConstructorCandidate) {
	for ci := range constructors {
		for pi := range constructors[ci].Parameters {
			param := &constructors[ci].Parameters[pi]
			if param.RuntimeValueExpr != "" {
				continue
			}
			candidates := runtimeval.Lookup(param.GoType)
			if len(candidates) == 0 {
				continue
			}
			param.RuntimeValueExpr = candidates[0].Expression
			param.Imports = append([]string(nil), candidates[0].Imports...)
		}
	}
}

func wrapperUsesSyntheticHTTPTransport(targets []WrapperTarget, constructors []ConstructorCandidate) bool {
	for _, target := range targets {
		for _, param := range target.Parameters {
			if runtimeExprUsesSyntheticHTTPTransport(param.RuntimeValueExpr) {
				return true
			}
		}
	}
	for _, constructor := range constructors {
		for _, param := range constructor.Parameters {
			if runtimeExprUsesSyntheticHTTPTransport(constructorParamRuntimeValueExpr(param)) {
				return true
			}
		}
	}
	return false
}

func runtimeExprUsesSyntheticHTTPTransport(expr string) bool {
	return strings.Contains(expr, "shatterHTTPClient()") || strings.Contains(expr, "shatterHTTPTransport()")
}

func writeSyntheticHTTPClientHelper(b *strings.Builder) {
	b.WriteString("type shatterHTTPRoundTripper func(*http.Request) (*http.Response, error)\n\n")
	b.WriteString("func (f shatterHTTPRoundTripper) RoundTrip(req *http.Request) (*http.Response, error) {\n")
	b.WriteString("\treturn f(req)\n")
	b.WriteString("}\n\n")
	b.WriteString("func shatterHTTPTransport() http.RoundTripper {\n")
	b.WriteString("\treturn shatterHTTPRoundTripper(shatterHTTPRoundTrip)\n")
	b.WriteString("}\n\n")
	b.WriteString("func shatterHTTPClient() *http.Client {\n")
	b.WriteString("\treturn &http.Client{Transport: shatterHTTPTransport()}\n")
	b.WriteString("}\n\n")
	b.WriteString("func shatterHTTPRoundTrip(req *http.Request) (*http.Response, error) {\n")
	b.WriteString("\tstatusCode := http.StatusOK\n")
	b.WriteString("\tbody := \"null\\n\"\n")
	b.WriteString("\treturn &http.Response{\n")
	b.WriteString("\t\tStatusCode: statusCode,\n")
	b.WriteString("\t\tStatus:     fmt.Sprintf(\"%d %s\", statusCode, http.StatusText(statusCode)),\n")
	b.WriteString("\t\tHeader:     http.Header{\"Content-Type\": []string{\"application/json\"}},\n")
	b.WriteString("\t\tBody:       io.NopCloser(strings.NewReader(body)),\n")
	b.WriteString("\t\tRequest:    req,\n")
	b.WriteString("\t}, nil\n")
	b.WriteString("}\n\n")
}

func writeMapInputNormalizer(b *strings.Builder) {
	b.WriteString("func shatterNormalizeMapInput(raw json.RawMessage) json.RawMessage {\n")
	b.WriteString("\tvar decoded any\n")
	b.WriteString("\tdec := json.NewDecoder(strings.NewReader(string(raw)))\n")
	b.WriteString("\tdec.UseNumber()\n")
	b.WriteString("\tif err := dec.Decode(&decoded); err != nil {\n")
	b.WriteString("\t\treturn raw\n")
	b.WriteString("\t}\n")
	b.WriteString("\tnormalized, changed := shatterNormalizeMapValue(decoded)\n")
	b.WriteString("\tif !changed {\n")
	b.WriteString("\t\treturn raw\n")
	b.WriteString("\t}\n")
	b.WriteString("\tout, err := json.Marshal(normalized)\n")
	b.WriteString("\tif err != nil {\n")
	b.WriteString("\t\treturn raw\n")
	b.WriteString("\t}\n")
	b.WriteString("\treturn out\n")
	b.WriteString("}\n\n")
	b.WriteString("func shatterNormalizeMapValue(value any) (any, bool) {\n")
	b.WriteString("\tswitch typed := value.(type) {\n")
	b.WriteString("\tcase map[string]any:\n")
	b.WriteString("\t\tif len(typed) == 2 {\n")
	b.WriteString("\t\t\tkey, hasKey := typed[\"_key\"]\n")
	b.WriteString("\t\t\tmapValue, hasValue := typed[\"_value\"]\n")
	b.WriteString("\t\t\tif hasKey && hasValue {\n")
	b.WriteString("\t\t\t\tnormalizedValue, _ := shatterNormalizeMapValue(mapValue)\n")
	b.WriteString("\t\t\t\treturn map[string]any{shatterMapKeyString(key): normalizedValue}, true\n")
	b.WriteString("\t\t\t}\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\tchanged := false\n")
	b.WriteString("\t\tnormalized := make(map[string]any, len(typed))\n")
	b.WriteString("\t\tfor key, mapValue := range typed {\n")
	b.WriteString("\t\t\tnormalizedValue, valueChanged := shatterNormalizeMapValue(mapValue)\n")
	b.WriteString("\t\t\tchanged = changed || valueChanged\n")
	b.WriteString("\t\t\tnormalized[key] = normalizedValue\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\treturn normalized, changed\n")
	b.WriteString("\tcase []any:\n")
	b.WriteString("\t\tchanged := false\n")
	b.WriteString("\t\tnormalized := make([]any, len(typed))\n")
	b.WriteString("\t\tfor i, item := range typed {\n")
	b.WriteString("\t\t\tnormalizedItem, itemChanged := shatterNormalizeMapValue(item)\n")
	b.WriteString("\t\t\tchanged = changed || itemChanged\n")
	b.WriteString("\t\t\tnormalized[i] = normalizedItem\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\treturn normalized, changed\n")
	b.WriteString("\tdefault:\n")
	b.WriteString("\t\treturn value, false\n")
	b.WriteString("\t}\n")
	b.WriteString("}\n\n")
	b.WriteString("func shatterMapKeyString(value any) string {\n")
	b.WriteString("\tif value == nil {\n")
	b.WriteString("\t\treturn \"null\"\n")
	b.WriteString("\t}\n")
	b.WriteString("\tif key, ok := value.(string); ok {\n")
	b.WriteString("\t\treturn key\n")
	b.WriteString("\t}\n")
	b.WriteString("\treturn fmt.Sprint(value)\n")
	b.WriteString("}\n\n")
}

func writeTimeInputNormalizer(b *strings.Builder) {
	b.WriteString("func shatterNormalizeTimeInput(raw json.RawMessage) json.RawMessage {\n")
	b.WriteString("\tvar decoded any\n")
	b.WriteString("\tdec := json.NewDecoder(strings.NewReader(string(raw)))\n")
	b.WriteString("\tdec.UseNumber()\n")
	b.WriteString("\tif err := dec.Decode(&decoded); err != nil {\n")
	b.WriteString("\t\treturn raw\n")
	b.WriteString("\t}\n")
	b.WriteString("\tnormalized, changed := shatterNormalizeTimeValue(decoded)\n")
	b.WriteString("\tif !changed {\n")
	b.WriteString("\t\treturn raw\n")
	b.WriteString("\t}\n")
	b.WriteString("\tout, err := json.Marshal(normalized)\n")
	b.WriteString("\tif err != nil {\n")
	b.WriteString("\t\treturn raw\n")
	b.WriteString("\t}\n")
	b.WriteString("\treturn out\n")
	b.WriteString("}\n\n")
	b.WriteString("func shatterNormalizeTimeValue(value any) (any, bool) {\n")
	b.WriteString("\tswitch typed := value.(type) {\n")
	b.WriteString("\tcase map[string]any:\n")
	b.WriteString("\t\tif encoded, ok := shatterDateMarkerString(typed); ok {\n")
	b.WriteString("\t\t\treturn encoded, true\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\tchanged := false\n")
	b.WriteString("\t\tnormalized := make(map[string]any, len(typed))\n")
	b.WriteString("\t\tfor key, mapValue := range typed {\n")
	b.WriteString("\t\t\tnormalizedValue, valueChanged := shatterNormalizeTimeValue(mapValue)\n")
	b.WriteString("\t\t\tchanged = changed || valueChanged\n")
	b.WriteString("\t\t\tnormalized[key] = normalizedValue\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\treturn normalized, changed\n")
	b.WriteString("\tcase []any:\n")
	b.WriteString("\t\tchanged := false\n")
	b.WriteString("\t\tnormalized := make([]any, len(typed))\n")
	b.WriteString("\t\tfor i, item := range typed {\n")
	b.WriteString("\t\t\tnormalizedItem, itemChanged := shatterNormalizeTimeValue(item)\n")
	b.WriteString("\t\t\tchanged = changed || itemChanged\n")
	b.WriteString("\t\t\tnormalized[i] = normalizedItem\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\treturn normalized, changed\n")
	b.WriteString("\tdefault:\n")
	b.WriteString("\t\treturn value, false\n")
	b.WriteString("\t}\n")
	b.WriteString("}\n\n")
	b.WriteString("func shatterDateMarkerString(value map[string]any) (string, bool) {\n")
	b.WriteString("\ttag, ok := value[\"__complex_type\"].(string)\n")
	b.WriteString("\tif !ok || (tag != \"date\" && tag != \"date_time\") {\n")
	b.WriteString("\t\treturn \"\", false\n")
	b.WriteString("\t}\n")
	b.WriteString("\tms, ok := value[\"value\"].(float64)\n")
	b.WriteString("\tif ok {\n")
	b.WriteString("\t\treturn time.UnixMilli(int64(ms)).UTC().Format(time.RFC3339Nano), true\n")
	b.WriteString("\t}\n")
	b.WriteString("\tn, ok := value[\"value\"].(json.Number)\n")
	b.WriteString("\tif !ok {\n")
	b.WriteString("\t\treturn \"\", false\n")
	b.WriteString("\t}\n")
	b.WriteString("\tmsInt, err := n.Int64()\n")
	b.WriteString("\tif err != nil {\n")
	b.WriteString("\t\treturn \"\", false\n")
	b.WriteString("\t}\n")
	b.WriteString("\treturn time.UnixMilli(msInt).UTC().Format(time.RFC3339Nano), true\n")
	b.WriteString("}\n\n")
}

func writeFuncInputNormalizer(b *strings.Builder) {
	b.WriteString("func shatterNormalizeFuncInput(raw json.RawMessage, sample any) json.RawMessage {\n")
	b.WriteString("\tvar decoded any\n")
	b.WriteString("\tdec := json.NewDecoder(strings.NewReader(string(raw)))\n")
	b.WriteString("\tdec.UseNumber()\n")
	b.WriteString("\tif err := dec.Decode(&decoded); err != nil {\n")
	b.WriteString("\t\treturn raw\n")
	b.WriteString("\t}\n")
	b.WriteString("\ttyp := reflect.TypeOf(sample)\n")
	b.WriteString("\tif typ == nil {\n")
	b.WriteString("\t\treturn raw\n")
	b.WriteString("\t}\n")
	b.WriteString("\tnormalized, changed := shatterNormalizeFuncValue(decoded, typ)\n")
	b.WriteString("\tif !changed {\n")
	b.WriteString("\t\treturn raw\n")
	b.WriteString("\t}\n")
	b.WriteString("\tout, err := json.Marshal(normalized)\n")
	b.WriteString("\tif err != nil {\n")
	b.WriteString("\t\treturn raw\n")
	b.WriteString("\t}\n")
	b.WriteString("\treturn out\n")
	b.WriteString("}\n\n")
	b.WriteString("func shatterNormalizeFuncValue(value any, typ reflect.Type) (any, bool) {\n")
	b.WriteString("\tif typ == nil {\n")
	b.WriteString("\t\treturn value, false\n")
	b.WriteString("\t}\n")
	b.WriteString("\tfor typ.Kind() == reflect.Pointer {\n")
	b.WriteString("\t\ttyp = typ.Elem()\n")
	b.WriteString("\t}\n")
	b.WriteString("\tswitch typ.Kind() {\n")
	b.WriteString("\tcase reflect.Func:\n")
	b.WriteString("\t\treturn nil, value != nil\n")
	b.WriteString("\tcase reflect.Struct:\n")
	b.WriteString("\t\tobject, ok := value.(map[string]any)\n")
	b.WriteString("\t\tif !ok {\n")
	b.WriteString("\t\t\treturn value, false\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\tvar normalized map[string]any\n")
	b.WriteString("\t\tchanged := false\n")
	b.WriteString("\t\tfor i := 0; i < typ.NumField(); i++ {\n")
	b.WriteString("\t\t\tfield := typ.Field(i)\n")
	b.WriteString("\t\t\tif field.PkgPath != \"\" {\n")
	b.WriteString("\t\t\t\tcontinue\n")
	b.WriteString("\t\t\t}\n")
	b.WriteString("\t\t\tnames := shatterJSONFieldNames(field)\n")
	b.WriteString("\t\t\tif len(names) == 0 {\n")
	b.WriteString("\t\t\t\tcontinue\n")
	b.WriteString("\t\t\t}\n")
	b.WriteString("\t\t\tif shatterFuncFieldType(field.Type) {\n")
	b.WriteString("\t\t\t\tfor _, name := range names {\n")
	b.WriteString("\t\t\t\t\tif _, ok := object[name]; ok {\n")
	b.WriteString("\t\t\t\t\t\tif normalized == nil {\n")
	b.WriteString("\t\t\t\t\t\t\tnormalized = shatterCopyObject(object)\n")
	b.WriteString("\t\t\t\t\t\t}\n")
	b.WriteString("\t\t\t\t\t\tdelete(normalized, name)\n")
	b.WriteString("\t\t\t\t\t\tchanged = true\n")
	b.WriteString("\t\t\t\t\t}\n")
	b.WriteString("\t\t\t\t}\n")
	b.WriteString("\t\t\t\tcontinue\n")
	b.WriteString("\t\t\t}\n")
	b.WriteString("\t\t\tfor _, name := range names {\n")
	b.WriteString("\t\t\t\tfieldValue, ok := object[name]\n")
	b.WriteString("\t\t\t\tif !ok {\n")
	b.WriteString("\t\t\t\t\tcontinue\n")
	b.WriteString("\t\t\t\t}\n")
	b.WriteString("\t\t\t\tnormalizedValue, valueChanged := shatterNormalizeFuncValue(fieldValue, field.Type)\n")
	b.WriteString("\t\t\t\tif valueChanged {\n")
	b.WriteString("\t\t\t\t\tif normalized == nil {\n")
	b.WriteString("\t\t\t\t\t\tnormalized = shatterCopyObject(object)\n")
	b.WriteString("\t\t\t\t\t}\n")
	b.WriteString("\t\t\t\t\tnormalized[name] = normalizedValue\n")
	b.WriteString("\t\t\t\t\tchanged = true\n")
	b.WriteString("\t\t\t\t}\n")
	b.WriteString("\t\t\t}\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\tif !changed {\n")
	b.WriteString("\t\t\treturn value, false\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\treturn normalized, true\n")
	b.WriteString("\tcase reflect.Slice, reflect.Array:\n")
	b.WriteString("\t\titems, ok := value.([]any)\n")
	b.WriteString("\t\tif !ok {\n")
	b.WriteString("\t\t\treturn value, false\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\tchanged := false\n")
	b.WriteString("\t\tnormalized := make([]any, len(items))\n")
	b.WriteString("\t\tfor i, item := range items {\n")
	b.WriteString("\t\t\tnormalizedItem, itemChanged := shatterNormalizeFuncValue(item, typ.Elem())\n")
	b.WriteString("\t\t\tchanged = changed || itemChanged\n")
	b.WriteString("\t\t\tnormalized[i] = normalizedItem\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\treturn normalized, changed\n")
	b.WriteString("\tcase reflect.Map:\n")
	b.WriteString("\t\tobject, ok := value.(map[string]any)\n")
	b.WriteString("\t\tif !ok {\n")
	b.WriteString("\t\t\treturn value, false\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\tchanged := false\n")
	b.WriteString("\t\tnormalized := make(map[string]any, len(object))\n")
	b.WriteString("\t\tfor key, mapValue := range object {\n")
	b.WriteString("\t\t\tnormalizedValue, valueChanged := shatterNormalizeFuncValue(mapValue, typ.Elem())\n")
	b.WriteString("\t\t\tchanged = changed || valueChanged\n")
	b.WriteString("\t\t\tnormalized[key] = normalizedValue\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\treturn normalized, changed\n")
	b.WriteString("\tdefault:\n")
	b.WriteString("\t\treturn value, false\n")
	b.WriteString("\t}\n")
	b.WriteString("}\n\n")
	b.WriteString("func shatterFuncFieldType(typ reflect.Type) bool {\n")
	b.WriteString("\tfor typ.Kind() == reflect.Pointer {\n")
	b.WriteString("\t\ttyp = typ.Elem()\n")
	b.WriteString("\t}\n")
	b.WriteString("\treturn typ.Kind() == reflect.Func\n")
	b.WriteString("}\n\n")
	b.WriteString("func shatterJSONFieldNames(field reflect.StructField) []string {\n")
	b.WriteString("\ttag := field.Tag.Get(\"json\")\n")
	b.WriteString("\tif tag == \"-\" {\n")
	b.WriteString("\t\treturn nil\n")
	b.WriteString("\t}\n")
	b.WriteString("\tname := tag\n")
	b.WriteString("\tif comma := strings.IndexByte(name, ','); comma >= 0 {\n")
	b.WriteString("\t\tname = name[:comma]\n")
	b.WriteString("\t}\n")
	b.WriteString("\tif name == \"\" {\n")
	b.WriteString("\t\treturn []string{field.Name}\n")
	b.WriteString("\t}\n")
	b.WriteString("\tif name == field.Name {\n")
	b.WriteString("\t\treturn []string{name}\n")
	b.WriteString("\t}\n")
	b.WriteString("\treturn []string{name, field.Name}\n")
	b.WriteString("}\n\n")
	b.WriteString("func shatterCopyObject(object map[string]any) map[string]any {\n")
	b.WriteString("\tcopy := make(map[string]any, len(object))\n")
	b.WriteString("\tfor key, value := range object {\n")
	b.WriteString("\t\tcopy[key] = value\n")
	b.WriteString("\t}\n")
	b.WriteString("\treturn copy\n")
	b.WriteString("}\n\n")
}

func writeRuntimeValueInputNormalizer(b *strings.Builder) {
	b.WriteString("func shatterNormalizeRuntimeValueInput(raw json.RawMessage, sample any) json.RawMessage {\n")
	b.WriteString("\tvar decoded any\n")
	b.WriteString("\tdec := json.NewDecoder(strings.NewReader(string(raw)))\n")
	b.WriteString("\tdec.UseNumber()\n")
	b.WriteString("\tif err := dec.Decode(&decoded); err != nil {\n")
	b.WriteString("\t\treturn raw\n")
	b.WriteString("\t}\n")
	b.WriteString("\ttyp := reflect.TypeOf(sample)\n")
	b.WriteString("\tif typ == nil {\n")
	b.WriteString("\t\treturn raw\n")
	b.WriteString("\t}\n")
	b.WriteString("\tnormalized, changed := shatterNormalizeRuntimeValue(decoded, typ)\n")
	b.WriteString("\tif !changed {\n")
	b.WriteString("\t\treturn raw\n")
	b.WriteString("\t}\n")
	b.WriteString("\tout, err := json.Marshal(normalized)\n")
	b.WriteString("\tif err != nil {\n")
	b.WriteString("\t\treturn raw\n")
	b.WriteString("\t}\n")
	b.WriteString("\treturn out\n")
	b.WriteString("}\n\n")
	b.WriteString("func shatterNormalizeRuntimeValue(value any, typ reflect.Type) (any, bool) {\n")
	b.WriteString("\tif typ == nil {\n")
	b.WriteString("\t\treturn value, false\n")
	b.WriteString("\t}\n")
	b.WriteString("\tfor typ.Kind() == reflect.Pointer {\n")
	b.WriteString("\t\ttyp = typ.Elem()\n")
	b.WriteString("\t}\n")
	b.WriteString("\tswitch typ.Kind() {\n")
	b.WriteString("\tcase reflect.Struct:\n")
	b.WriteString("\t\tobject, ok := value.(map[string]any)\n")
	b.WriteString("\t\tif !ok {\n")
	b.WriteString("\t\t\treturn value, false\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\tvar normalized map[string]any\n")
	b.WriteString("\t\tchanged := false\n")
	b.WriteString("\t\tfor i := 0; i < typ.NumField(); i++ {\n")
	b.WriteString("\t\t\tfield := typ.Field(i)\n")
	b.WriteString("\t\t\tif field.PkgPath != \"\" {\n")
	b.WriteString("\t\t\t\tcontinue\n")
	b.WriteString("\t\t\t}\n")
	b.WriteString("\t\t\tnames := shatterRuntimeJSONFieldNames(field)\n")
	b.WriteString("\t\t\tif len(names) == 0 {\n")
	b.WriteString("\t\t\t\tcontinue\n")
	b.WriteString("\t\t\t}\n")
	b.WriteString("\t\t\tif shatterRuntimeValueFieldType(field.Type, make(map[reflect.Type]struct{})) {\n")
	b.WriteString("\t\t\t\tfor _, name := range names {\n")
	b.WriteString("\t\t\t\t\tif _, ok := object[name]; ok {\n")
	b.WriteString("\t\t\t\t\t\tif normalized == nil {\n")
	b.WriteString("\t\t\t\t\t\t\tnormalized = shatterRuntimeCopyObject(object)\n")
	b.WriteString("\t\t\t\t\t\t}\n")
	b.WriteString("\t\t\t\t\t\tdelete(normalized, name)\n")
	b.WriteString("\t\t\t\t\t\tchanged = true\n")
	b.WriteString("\t\t\t\t\t}\n")
	b.WriteString("\t\t\t\t}\n")
	b.WriteString("\t\t\t\tcontinue\n")
	b.WriteString("\t\t\t}\n")
	b.WriteString("\t\t\tfor _, name := range names {\n")
	b.WriteString("\t\t\t\tfieldValue, ok := object[name]\n")
	b.WriteString("\t\t\t\tif !ok {\n")
	b.WriteString("\t\t\t\t\tcontinue\n")
	b.WriteString("\t\t\t\t}\n")
	b.WriteString("\t\t\t\tnormalizedValue, valueChanged := shatterNormalizeRuntimeValue(fieldValue, field.Type)\n")
	b.WriteString("\t\t\t\tif valueChanged {\n")
	b.WriteString("\t\t\t\t\tif normalized == nil {\n")
	b.WriteString("\t\t\t\t\t\tnormalized = shatterRuntimeCopyObject(object)\n")
	b.WriteString("\t\t\t\t\t}\n")
	b.WriteString("\t\t\t\t\tnormalized[name] = normalizedValue\n")
	b.WriteString("\t\t\t\t\tchanged = true\n")
	b.WriteString("\t\t\t\t}\n")
	b.WriteString("\t\t\t}\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\tif !changed {\n")
	b.WriteString("\t\t\treturn value, false\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\treturn normalized, true\n")
	b.WriteString("\tcase reflect.Slice, reflect.Array:\n")
	b.WriteString("\t\titems, ok := value.([]any)\n")
	b.WriteString("\t\tif !ok {\n")
	b.WriteString("\t\t\treturn value, false\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\tchanged := false\n")
	b.WriteString("\t\tnormalized := make([]any, len(items))\n")
	b.WriteString("\t\tfor i, item := range items {\n")
	b.WriteString("\t\t\tnormalizedItem, itemChanged := shatterNormalizeRuntimeValue(item, typ.Elem())\n")
	b.WriteString("\t\t\tchanged = changed || itemChanged\n")
	b.WriteString("\t\t\tnormalized[i] = normalizedItem\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\treturn normalized, changed\n")
	b.WriteString("\tcase reflect.Map:\n")
	b.WriteString("\t\tobject, ok := value.(map[string]any)\n")
	b.WriteString("\t\tif !ok {\n")
	b.WriteString("\t\t\treturn value, false\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\tchanged := false\n")
	b.WriteString("\t\tnormalized := make(map[string]any, len(object))\n")
	b.WriteString("\t\tfor key, mapValue := range object {\n")
	b.WriteString("\t\t\tnormalizedValue, valueChanged := shatterNormalizeRuntimeValue(mapValue, typ.Elem())\n")
	b.WriteString("\t\t\tchanged = changed || valueChanged\n")
	b.WriteString("\t\t\tnormalized[key] = normalizedValue\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\t\treturn normalized, changed\n")
	b.WriteString("\tdefault:\n")
	b.WriteString("\t\treturn value, false\n")
	b.WriteString("\t}\n")
	b.WriteString("}\n\n")
	b.WriteString("func shatterRuntimeValueFieldType(typ reflect.Type, seen map[reflect.Type]struct{}) bool {\n")
	b.WriteString("\tif typ == nil {\n")
	b.WriteString("\t\treturn false\n")
	b.WriteString("\t}\n")
	b.WriteString("\tif _, ok := seen[typ]; ok {\n")
	b.WriteString("\t\treturn false\n")
	b.WriteString("\t}\n")
	b.WriteString("\tseen[typ] = struct{}{}\n")
	b.WriteString("\tfor typ.Kind() == reflect.Pointer {\n")
	b.WriteString("\t\ttyp = typ.Elem()\n")
	b.WriteString("\t}\n")
	b.WriteString("\tif shatterRuntimeValueNamedType(typ) {\n")
	b.WriteString("\t\treturn true\n")
	b.WriteString("\t}\n")
	b.WriteString("\tswitch typ.Kind() {\n")
	b.WriteString("\tcase reflect.Struct:\n")
	b.WriteString("\t\tfor i := 0; i < typ.NumField(); i++ {\n")
	b.WriteString("\t\t\tif shatterRuntimeValueFieldType(typ.Field(i).Type, seen) {\n")
	b.WriteString("\t\t\t\treturn true\n")
	b.WriteString("\t\t\t}\n")
	b.WriteString("\t\t}\n")
	b.WriteString("\tcase reflect.Slice, reflect.Array, reflect.Map:\n")
	b.WriteString("\t\treturn shatterRuntimeValueFieldType(typ.Elem(), seen)\n")
	b.WriteString("\t}\n")
	b.WriteString("\treturn false\n")
	b.WriteString("}\n\n")
	b.WriteString("func shatterRuntimeValueNamedType(typ reflect.Type) bool {\n")
	b.WriteString("\treturn typ.PkgPath() == \"github.com/tetratelabs/wazero\" && typ.Name() == \"CompiledModule\"\n")
	b.WriteString("}\n\n")
	b.WriteString("func shatterRuntimeJSONFieldNames(field reflect.StructField) []string {\n")
	b.WriteString("\ttag := field.Tag.Get(\"json\")\n")
	b.WriteString("\tif tag == \"-\" {\n")
	b.WriteString("\t\treturn nil\n")
	b.WriteString("\t}\n")
	b.WriteString("\tname := tag\n")
	b.WriteString("\tif comma := strings.IndexByte(name, ','); comma >= 0 {\n")
	b.WriteString("\t\tname = name[:comma]\n")
	b.WriteString("\t}\n")
	b.WriteString("\tif name == \"\" {\n")
	b.WriteString("\t\treturn []string{field.Name}\n")
	b.WriteString("\t}\n")
	b.WriteString("\tif name == field.Name {\n")
	b.WriteString("\t\treturn []string{name}\n")
	b.WriteString("\t}\n")
	b.WriteString("\treturn []string{name, field.Name}\n")
	b.WriteString("}\n\n")
	b.WriteString("func shatterRuntimeCopyObject(object map[string]any) map[string]any {\n")
	b.WriteString("\tcopy := make(map[string]any, len(object))\n")
	b.WriteString("\tfor key, value := range object {\n")
	b.WriteString("\t\tcopy[key] = value\n")
	b.WriteString("\t}\n")
	b.WriteString("\treturn copy\n")
	b.WriteString("}\n\n")
}

// constructorInputArgExpr builds the argument expression string for a
// constructor call. For parameterless constructors returns "". For
// parameterized constructors returns the temporary variables populated from
// the input prefix immediately before the call.
func constructorInputArgExpr(c ConstructorCandidate) string {
	if len(c.Parameters) == 0 {
		return ""
	}
	args := make([]string, len(c.Parameters))
	for i := range c.Parameters {
		args[i] = fmt.Sprintf("_shatterCtorArg%d", i)
	}
	return strings.Join(args, ", ")
}

// writeDurationParamDeserialization emits a time.Duration-specific decode
// block (str-is5g). The canonical wire format is integer nanoseconds — that
// is what time.Duration's default (int64) UnmarshalJSON consumes and what
// the Go planner emits as ValuePlan literals. The Rust core's random input
// generator emits the legacy `{"__complex_type":"duration","ms":N}` shape
// shared with the TS frontend; rather than push canonicalisation across the
// crate boundary, the wrapper accepts both forms here. An integer decode is
// tried first; on UnmarshalTypeError (the object shape) it falls back to
// reading `ms` from the tagged object and converting milliseconds to
// nanoseconds. Any other shape preserves the original integer-decode error
// so the failure message stays specific.
func writeDurationParamDeserialization(b *strings.Builder, name string, idx int, indent string) {
	fmt.Fprintf(b, "%svar %s time.Duration\n", indent, name)
	fmt.Fprintf(b, "%sif %d < len(_shatterInputs) {\n", indent, idx)
	fmt.Fprintf(b, "%s\tif _e := json.Unmarshal(_shatterInputs[%d], &%s); _e != nil {\n", indent, idx, name)
	fmt.Fprintf(b, "%s\t\tvar _shatterDur struct {\n", indent)
	fmt.Fprintf(b, "%s\t\t\tComplexType string `json:\"__complex_type\"`\n", indent)
	fmt.Fprintf(b, "%s\t\t\tMs          *int64 `json:\"ms\"`\n", indent)
	fmt.Fprintf(b, "%s\t\t}\n", indent)
	fmt.Fprintf(b, "%s\t\tif _e2 := json.Unmarshal(_shatterInputs[%d], &_shatterDur); _e2 != nil || _shatterDur.Ms == nil || _shatterDur.ComplexType != \"duration\" {\n", indent, idx)
	fmt.Fprintf(b, "%s\t\t\treturn nil, fmt.Errorf(\"param %s: %%w\", _e)\n", indent, name)
	fmt.Fprintf(b, "%s\t\t}\n", indent)
	fmt.Fprintf(b, "%s\t\t%s = time.Duration(*_shatterDur.Ms) * time.Millisecond\n", indent, name)
	fmt.Fprintf(b, "%s\t}\n", indent)
	fmt.Fprintf(b, "%s}\n", indent)
}

// writeErrorParamDeserialization emits a decode block for a bare builtin
// `error`-interface parameter (str-jn9r0), mirroring
// writeDurationParamDeserialization. The Go analyzer maps builtin `error` to
// ComplexKind "error" (analyzer.go complexKindFromNamed) and the Rust core's
// random generator emits the cross-frontend shape
// `{"__complex_type":"error","class":...,"message":m}` (input_gen.rs
// generate_error). A bare `error` interface cannot be json.Unmarshaled
// directly, so this block: (a) tries a plain decode first — JSON `null`
// decodes into the interface as a nil error with no error, giving the caller
// the nil branch for free; (b) on any decode error, falls back to reading the
// tagged object and reconstructing `errors.New(message)` (the `class` field is
// intentionally ignored — no typed-error reconstruction yet, str-kvzh7).
// Any other shape preserves the original plain-decode error so the failure
// message stays specific.
func writeErrorParamDeserialization(b *strings.Builder, name string, idx int, indent string) {
	fmt.Fprintf(b, "%svar %s error\n", indent, name)
	fmt.Fprintf(b, "%sif %d < len(_shatterInputs) {\n", indent, idx)
	fmt.Fprintf(b, "%s\tif _e := json.Unmarshal(_shatterInputs[%d], &%s); _e != nil {\n", indent, idx, name)
	fmt.Fprintf(b, "%s\t\tvar _shatterErr struct {\n", indent)
	fmt.Fprintf(b, "%s\t\t\tComplexType string  `json:\"__complex_type\"`\n", indent)
	fmt.Fprintf(b, "%s\t\t\tMessage     *string `json:\"message\"`\n", indent)
	fmt.Fprintf(b, "%s\t\t}\n", indent)
	fmt.Fprintf(b, "%s\t\tif _e2 := json.Unmarshal(_shatterInputs[%d], &_shatterErr); _e2 != nil || _shatterErr.Message == nil || _shatterErr.ComplexType != \"error\" {\n", indent, idx)
	fmt.Fprintf(b, "%s\t\t\treturn nil, fmt.Errorf(\"param %s: %%w\", _e)\n", indent, name)
	fmt.Fprintf(b, "%s\t\t}\n", indent)
	fmt.Fprintf(b, "%s\t\t%s = errors.New(*_shatterErr.Message)\n", indent, name)
	fmt.Fprintf(b, "%s\t}\n", indent)
	fmt.Fprintf(b, "%s}\n", indent)
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
		switch {
		case wrapperTargetReturnsOnlyError(t):
			fmt.Fprintf(b, "%s_invokeErr := %s\n", indent, callExpr)
			fmt.Fprintf(b, "%sreturn nil, _invokeErr\n", indent)
		case wrapperTargetHasTrailingError(t):
			blanks := ""
			if t.ResultCount > 2 {
				blanks = strings.Repeat(", _", t.ResultCount-2)
			}
			fmt.Fprintf(b, "%s_result%s, _invokeErr := %s\n", indent, blanks, callExpr)
			fmt.Fprintf(b, "%sif _invokeErr != nil {\n", indent)
			fmt.Fprintf(b, "%s\treturn nil, _invokeErr\n", indent)
			fmt.Fprintf(b, "%s}\n", indent)
			fmt.Fprintf(b, "%sreturn _result, nil\n", indent)
		case t.ResultCount > 1:
			blanks := strings.Repeat(", _", t.ResultCount-1)
			fmt.Fprintf(b, "%s_result%s := %s\n", indent, blanks, callExpr)
			fmt.Fprintf(b, "%sreturn _result, nil\n", indent)
		default:
			fmt.Fprintf(b, "%s_result := %s\n", indent, callExpr)
			fmt.Fprintf(b, "%sreturn _result, nil\n", indent)
		}
	} else {
		fmt.Fprintf(b, "%s%s\n", indent, callExpr)
		fmt.Fprintf(b, "%sreturn nil, nil\n", indent)
	}
}

func wrapperTargetReturnsOnlyError(t WrapperTarget) bool {
	resultTypes := wrapperTargetResultTypes(t)
	return len(resultTypes) == 1 && resultTypes[0] == "error"
}

func wrapperTargetHasTrailingError(t WrapperTarget) bool {
	resultTypes := wrapperTargetResultTypes(t)
	return len(resultTypes) > 1 && resultTypes[len(resultTypes)-1] == "error"
}

func wrapperTargetResultTypes(t WrapperTarget) []string {
	if len(t.ResultGoTypes) == t.ResultCount {
		return t.ResultGoTypes
	}
	if t.ResultCount == 1 && t.ResultGoType != "" {
		return []string{t.ResultGoType}
	}
	return nil
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

func collectReceiverMapFields(
	pkg *packages.Package,
	recvType string,
	pkgTypesPath string,
	importSet map[string]struct{},
) []ReceiverMapField {
	if pkg == nil || pkg.Types == nil || recvType == "" {
		return nil
	}
	scope := pkg.Types.Scope()
	if scope == nil {
		return nil
	}
	obj := scope.Lookup(recvType)
	if obj == nil {
		return nil
	}
	named, ok := obj.Type().(*types.Named)
	if !ok {
		return nil
	}
	st, ok := named.Underlying().(*types.Struct)
	if !ok {
		return nil
	}
	fields := make([]ReceiverMapField, 0)
	for i := 0; i < st.NumFields(); i++ {
		field := st.Field(i)
		if _, ok := field.Type().Underlying().(*types.Map); ok {
			fields = append(fields, ReceiverMapField{
				Name:   field.Name(),
				GoType: goTypeStringForReceiverField(field.Type(), pkgTypesPath, importSet),
			})
			continue
		}
		if !field.Exported() && receiverFieldNeedsNonMapInitialization(field.Type()) {
			return nil
		}
	}
	return fields
}

func receiverFieldNeedsNonMapInitialization(t types.Type) bool {
	switch t.Underlying().(type) {
	case *types.Chan, *types.Signature, *types.Interface, *types.Pointer:
		return true
	default:
		return false
	}
}

func goTypeStringForReceiverField(t types.Type, pkgTypesPath string, importSet map[string]struct{}) string {
	return types.TypeString(t, func(pkg *types.Package) string {
		if pkg == nil || pkg.Path() == "" || pkg.Path() == pkgTypesPath {
			return ""
		}
		if importSet != nil {
			importSet[pkg.Path()] = struct{}{}
		}
		return pkg.Name()
	})
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
	// str-jeen.79: use the type-checker's actual package path (pkg.Types.Path())
	// for the qualifier comparison so that same-name sibling packages are not
	// mistaken for the current package. We prefer pkg.Types.Path() over
	// pkg.PkgPath because test helpers sometimes set PkgPath to a different
	// value than the path given to conf.Check, creating a mismatch. In
	// production (packages.Load) both values are always the full import path.
	// Fall back to pkg.PkgPath when pkg.Types is nil.
	pkgTypesPath := pkg.PkgPath
	if pkg.Types != nil {
		pkgTypesPath = pkg.Types.Path()
	}
	receiverMapFields := collectReceiverMapFields(pkg, recvType, pkgTypesPath, importSet)
	params := extractWrapperParams(fn, pkg.TypesInfo, pkg.Name, pkgTypesPath, importSet)
	// str-gxjs.1: bind runtime-value expressions for parameter types the
	// planner's registry can satisfy (context.Context → context.Background(),
	// http.ResponseWriter → httptest.NewRecorder(), …). The expression and
	// its required imports are recorded on the param + target so
	// writeParamDeserialization can emit a direct assignment instead of a
	// json.Unmarshal block. Without this, a target taking context.Context
	// would compile and link but the param would be the zero interface
	// value (`nil`), panicking on first use.
	applyRuntimeValueBindingsForPackage(params, importSet, configuredRuntimeValuesForFunc(fn, pkg), pkg.Name)
	applyImportedConstructorBindingsForPackage(fn, pkg, params, importSet, pkgTypesPath)
	typeParams := extractWrapperTypeParams(fn)

	hasResult := false
	var resultGoType string
	var resultGoTypes []string
	resultCount := 0
	if fn.Type.Results != nil && len(fn.Type.Results.List) > 0 {
		hasResult = true
		resultGoType = wrapperGoType(fn.Type.Results.List[0].Type, pkg.TypesInfo, pkg.Name, pkgTypesPath, nil)
		for _, field := range fn.Type.Results.List {
			goType := wrapperGoType(field.Type, pkg.TypesInfo, pkg.Name, pkgTypesPath, nil)
			if len(field.Names) == 0 {
				resultCount++
				resultGoTypes = append(resultGoTypes, goType)
			} else {
				resultCount += len(field.Names)
				for range field.Names {
					resultGoTypes = append(resultGoTypes, goType)
				}
			}
		}
	}

	imports := make([]string, 0, len(importSet))
	for importPath := range importSet {
		imports = append(imports, importPath)
	}
	sort.Strings(imports)

	return &WrapperTarget{
		ID:                id,
		SymbolName:        fn.Name.Name,
		Kind:              kind,
		ReceiverType:      recvType,
		IsPointerRecv:     isPtr,
		ReceiverMapFields: receiverMapFields,
		Parameters:        params,
		TypeParams:        typeParams,
		HasResult:         hasResult,
		ResultGoType:      resultGoType,
		ResultGoTypes:     resultGoTypes,
		ResultCount:       resultCount,
		Imports:           imports,
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

func appendStringIfMissing(values []string, value string) []string {
	for _, existing := range values {
		if existing == value {
			return values
		}
	}
	return append(values, value)
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

// applyRuntimeValueBindings consults the planner's runtime-value registry
// for each WrapperParam and records the first candidate expression when
// the param's GoType matches a registered entry. The required imports
// are added to importSet so wrapper-gen emits them alongside the
// existing parameter-type imports. Variadic params are skipped because
// the registry today carries values for the scalar type (`io.Writer`)
// and not for its `...T` form, where slice-spread semantics would
// change the call shape (str-jeen.48).
//
// Multiple candidate entries (e.g. `io.Writer` has both `&bytes.Buffer{}`
// and `io.Discard`) collapse to the first one — the wrapper produces a
// single fixed expression per param at build time; per-input variation
// would require a wrapper-side switch and is outside this change.
func applyRuntimeValueBindings(params []WrapperParam, importSet map[string]struct{}, configuredValues ...map[string]config.GoRuntimeValueConfig) {
	var configured map[string]config.GoRuntimeValueConfig
	if len(configuredValues) > 0 {
		configured = configuredValues[0]
	}
	applyRuntimeValueBindingsForPackage(params, importSet, configured, "")
}

func applyRuntimeValueBindingsForPackage(
	params []WrapperParam,
	importSet map[string]struct{},
	configured map[string]config.GoRuntimeValueConfig,
	pkgName string,
) {
	for i := range params {
		if params[i].IsVariadic {
			continue
		}
		if cand, ok := runtimeval.LookupSymbolic(strings.TrimSpace(params[i].GoType)); ok {
			// str-e41w / str-ijtww: a symbolic-construction parameter (e.g. a
			// direct *http.Request) is built from a symbolic input slot in
			// writeParamDeserializationAtInputIndex, rather than bound to the
			// fixed runtime value. Leave RuntimeValueExpr empty so the param
			// consumes its input slot, and record the registry-declared imports
			// its construction needs.
			for _, imp := range cand.Imports {
				if imp != "" {
					importSet[imp] = struct{}{}
				}
			}
			continue
		}
		candidates := runtimeval.Lookup(params[i].GoType)
		if len(candidates) == 0 {
			if rv, ok := configuredRuntimeValue(params[i].GoType, configured, pkgName); ok {
				params[i].RuntimeValueExpr = rv.Expression
				for _, imp := range rv.Imports {
					if imp != "" {
						importSet[imp] = struct{}{}
					}
				}
				continue
			}
			// str-4cqz: function-typed parameters have no JSON
			// representation; attempting to unmarshal a JSON input into
			// a `func(...)` slot produces a "cannot unmarshal X into Go
			// value of type func(...)" error cluster on every iteration.
			// Bake `nil` as a deterministic stub so the wrapper compiles
			// without a JSON slot for the parameter. Target bodies that
			// invoke the nil callback surface as a regular panic, which
			// the outcome classifier reports as `runtime_failed` —
			// distinguishable from the prior structural unmarshal noise.
			if isFuncTypeSpelling(params[i].GoType) {
				params[i].RuntimeValueExpr = "nil"
			}
			continue
		}
		params[i].RuntimeValueExpr = candidates[0].Expression
		for _, imp := range candidates[0].Imports {
			if imp != "" {
				importSet[imp] = struct{}{}
			}
		}
	}
}

type constructorRuntimeBinding struct {
	Expression string
	Imports    []string
}

func applyImportedConstructorBindingsForPackage(
	fn *ast.FuncDecl,
	pkg *packages.Package,
	params []WrapperParam,
	importSet map[string]struct{},
	pkgPath string,
) {
	if fn == nil || fn.Type == nil || fn.Type.Params == nil || pkg == nil || pkg.TypesInfo == nil || len(params) == 0 {
		return
	}
	paramIndex := 0
	for _, field := range fn.Type.Params.List {
		fieldType := field.Type
		if _, ok := fieldType.(*ast.Ellipsis); ok {
			paramIndex += wrapperParamFieldCount(field)
			continue
		}
		for range wrapperParamFieldCount(field) {
			if paramIndex >= len(params) {
				return
			}
			if isSymbolicParam(params[paramIndex].GoType) {
				paramIndex++
				continue
			}
			if params[paramIndex].RuntimeValueExpr == "" {
				if binding, ok := importedParameterConstructorBinding(fieldType, pkg, pkgPath); ok {
					params[paramIndex].RuntimeValueExpr = binding.Expression
					for _, imp := range binding.Imports {
						if imp != "" {
							importSet[imp] = struct{}{}
						}
					}
				}
			}
			paramIndex++
		}
	}
}

func wrapperParamFieldCount(field *ast.Field) int {
	if field == nil || len(field.Names) == 0 {
		return 1
	}
	return len(field.Names)
}

func importedParameterConstructorBinding(
	expr ast.Expr,
	consumerPkg *packages.Package,
	consumerPkgPath string,
) (constructorRuntimeBinding, bool) {
	named, wantsPointer := resolveNamedConcreteWrapperType(expr, consumerPkg.TypesInfo)
	if named == nil || named.Obj() == nil || named.Obj().Pkg() == nil {
		return constructorRuntimeBinding{}, false
	}
	if _, isInterface := named.Underlying().(*types.Interface); isInterface {
		return constructorRuntimeBinding{}, false
	}
	defPkgPath := named.Obj().Pkg().Path()
	if defPkgPath == "" || defPkgPath == consumerPkgPath || defPkgPath == consumerPkg.PkgPath {
		return constructorRuntimeBinding{}, false
	}
	defPkg := consumerPkg.Imports[defPkgPath]
	if defPkg == nil || defPkg.TypesInfo == nil {
		return constructorRuntimeBinding{}, false
	}

	candidates := importedParameterConstructorsForType(defPkg, named.Obj().Name())
	if len(candidates) == 0 {
		return constructorRuntimeBinding{}, false
	}
	sort.SliceStable(candidates, func(i, j int) bool {
		if candidates[i].HasRuntime != candidates[j].HasRuntime {
			return candidates[i].HasRuntime
		}
		if candidates[i].FuncName != candidates[j].FuncName {
			return candidates[i].FuncName < candidates[j].FuncName
		}
		return strings.Join(candidates[i].ArgExprs, "\x00") < strings.Join(candidates[j].ArgExprs, "\x00")
	})
	pkgName := defPkg.Name
	if defPkg.Types != nil && defPkg.Types.Name() != "" {
		pkgName = defPkg.Types.Name()
	}
	constructor := candidates[0]
	typeExpr := pkgName + "." + named.Obj().Name()
	ctorCall := pkgName + "." + constructor.FuncName + "(" + strings.Join(constructor.ArgExprs, ", ") + ")"
	ctorExpr := importedParameterConstructorExpression(typeExpr, ctorCall, wantsPointer, constructor)
	imports := append([]string{defPkgPath}, constructor.Imports...)
	return constructorRuntimeBinding{
		Expression: ctorExpr,
		Imports:    sortedStrings(imports),
	}, true
}

type importedParameterConstructorCandidate struct {
	FuncName       string
	ArgExprs       []string
	Imports        []string
	HasRuntime     bool
	ReturnsPointer bool
	ReturnsError   bool
}

func importedParameterConstructorExpression(
	typeExpr string,
	ctorCall string,
	wantsPointer bool,
	constructor importedParameterConstructorCandidate,
) string {
	switch {
	case wantsPointer && constructor.ReturnsPointer && constructor.ReturnsError:
		return fmt.Sprintf("func() *%s { v, _ := %s; return v }()", typeExpr, ctorCall)
	case wantsPointer && constructor.ReturnsPointer:
		return ctorCall
	case wantsPointer && !constructor.ReturnsPointer && constructor.ReturnsError:
		return fmt.Sprintf("func() *%s { v, _ := %s; return &v }()", typeExpr, ctorCall)
	case wantsPointer && !constructor.ReturnsPointer:
		return fmt.Sprintf("func() *%s { v := %s; return &v }()", typeExpr, ctorCall)
	case !wantsPointer && constructor.ReturnsPointer && constructor.ReturnsError:
		return fmt.Sprintf("func() %s { v, _ := %s; if v == nil { return %s{} }; return *v }()", typeExpr, ctorCall, typeExpr)
	case !wantsPointer && constructor.ReturnsPointer:
		return fmt.Sprintf("func() %s { v := %s; if v == nil { return %s{} }; return *v }()", typeExpr, ctorCall, typeExpr)
	case constructor.ReturnsError:
		return fmt.Sprintf("func() %s { v, _ := %s; return v }()", typeExpr, ctorCall)
	default:
		return ctorCall
	}
}

func resolveNamedConcreteWrapperType(expr ast.Expr, info *types.Info) (*types.Named, bool) {
	if expr == nil || info == nil {
		return nil, false
	}
	wantsPointer := false
	if star, ok := expr.(*ast.StarExpr); ok {
		wantsPointer = true
		expr = star.X
	}
	tv, ok := info.Types[expr]
	if !ok || tv.Type == nil {
		return nil, wantsPointer
	}
	named, ok := tv.Type.(*types.Named)
	if !ok {
		return nil, wantsPointer
	}
	return named, wantsPointer
}

func importedParameterConstructorsForType(pkg *packages.Package, targetType string) []importedParameterConstructorCandidate {
	if pkg == nil || pkg.TypesInfo == nil {
		return nil
	}
	var candidates []importedParameterConstructorCandidate
	for _, file := range pkg.Syntax {
		if file == nil {
			continue
		}
		for _, decl := range file.Decls {
			fn, ok := decl.(*ast.FuncDecl)
			if !ok || fn.Body == nil || fn.Recv != nil {
				continue
			}
			if !isImportedParameterConstructorName(fn.Name.Name) {
				continue
			}
			returnsPointer, returnsError, ok := constructorReturnsImportedParamType(fn, pkg, targetType)
			if ok {
				if candidate, ok := importedParameterConstructorCandidateForFunc(fn, pkg); ok {
					candidate.ReturnsPointer = returnsPointer
					candidate.ReturnsError = returnsError
					candidates = append(candidates, candidate)
				}
			}
		}
	}
	return candidates
}

func importedParameterConstructorCandidateForFunc(fn *ast.FuncDecl, pkg *packages.Package) (importedParameterConstructorCandidate, bool) {
	candidate := importedParameterConstructorCandidate{FuncName: fn.Name.Name}
	params := importedConstructorParamsForFunc(fn, pkg)
	if len(params) == 0 {
		return candidate, true
	}

	for _, param := range params {
		arg, imports, runtimeValue, ok := importedConstructorArgExpression(param)
		if !ok {
			return importedParameterConstructorCandidate{}, false
		}
		candidate.ArgExprs = append(candidate.ArgExprs, arg)
		candidate.Imports = append(candidate.Imports, imports...)
		if runtimeValue {
			candidate.HasRuntime = true
		}
	}
	if !candidate.HasRuntime {
		return importedParameterConstructorCandidate{}, false
	}
	candidate.Imports = sortedStrings(candidate.Imports)
	return candidate, true
}

func importedConstructorParamsForFunc(fn *ast.FuncDecl, pkg *packages.Package) []ConstructorParam {
	if fn == nil || fn.Type == nil || fn.Type.Params == nil || pkg == nil {
		return nil
	}
	pkgTypesPath := pkg.PkgPath
	if pkg.Types != nil {
		pkgTypesPath = pkg.Types.Path()
	}
	var params []ConstructorParam
	index := 0
	for _, field := range fn.Type.Params.List {
		fieldType := field.Type
		if _, ok := fieldType.(*ast.Ellipsis); ok {
			index += wrapperParamFieldCount(field)
			continue
		}
		goType := wrapperGoType(fieldType, pkg.TypesInfo, pkg.Name, pkgTypesPath, nil)
		if len(field.Names) == 0 {
			params = append(params, ConstructorParam{
				Name:   syntheticParamName(index),
				GoType: goType,
			})
			index++
			continue
		}
		for _, name := range field.Names {
			paramName := name.Name
			if paramName == "" || paramName == "_" {
				paramName = syntheticParamName(index)
			}
			params = append(params, ConstructorParam{
				Name:   paramName,
				GoType: goType,
			})
			index++
		}
	}
	return params
}

func importedConstructorArgExpression(param ConstructorParam) (string, []string, bool, bool) {
	typeName := strings.TrimSpace(param.GoType)
	if typeName != "" {
		if candidates := runtimeval.Lookup(typeName); len(candidates) > 0 {
			return candidates[0].Expression, candidates[0].Imports, true, true
		}
		switch typeName {
		case "string":
			return importedConstructorStringArgExpression(param.Name), nil, false, true
		case "[]byte", "[]uint8":
			return "nil", nil, false, true
		case "bool":
			return "false", nil, false, true
		case "time.Duration":
			return "0", nil, false, true
		case "int", "int8", "int16", "int32", "int64",
			"uint", "uint8", "uint16", "uint32", "uint64",
			"float32", "float64":
			return "0", nil, false, true
		}
		if strings.HasSuffix(typeName, ".Duration") {
			return "0", nil, false, true
		}
	}
	return "", nil, false, false
}

func importedConstructorStringArgExpression(name string) string {
	lower := strings.ToLower(name)
	switch {
	case strings.Contains(lower, "url"),
		strings.Contains(lower, "endpoint"),
		strings.Contains(lower, "addr"),
		strings.Contains(lower, "address"),
		strings.Contains(lower, "base"):
		return `"http://127.0.0.1:0"`
	default:
		return `""`
	}
}

func isImportedParameterConstructorName(name string) bool {
	return strings.HasPrefix(name, "New") || strings.HasPrefix(name, "Default")
}

func constructorReturnsImportedParamType(fn *ast.FuncDecl, pkg *packages.Package, targetType string) (returnsPointer bool, returnsError bool, ok bool) {
	if fn == nil || fn.Type == nil || fn.Type.Results == nil {
		return false, false, false
	}
	var exprs []ast.Expr
	for _, field := range fn.Type.Results.List {
		count := wrapperParamFieldCount(field)
		for range count {
			exprs = append(exprs, field.Type)
		}
	}
	if len(exprs) == 0 || len(exprs) > 2 {
		return false, false, false
	}
	name, isPtr, same := sameWrapperPackageTypeName(exprs[0], pkg)
	if !same || name != targetType {
		return false, false, false
	}
	if len(exprs) == 2 {
		if !wrapperIsErrorExpr(exprs[1], pkg.TypesInfo) {
			return false, false, false
		}
		return isPtr, true, true
	}
	return isPtr, false, true
}

func sameWrapperPackageTypeName(expr ast.Expr, pkg *packages.Package) (string, bool, bool) {
	isPointer := false
	if star, ok := expr.(*ast.StarExpr); ok {
		isPointer = true
		expr = star.X
	}
	ident, ok := expr.(*ast.Ident)
	if !ok || pkg == nil || pkg.TypesInfo == nil {
		return "", false, false
	}
	obj := pkg.TypesInfo.Uses[ident]
	if obj == nil {
		obj = pkg.TypesInfo.Defs[ident]
	}
	tn, ok := obj.(*types.TypeName)
	if !ok || tn.Pkg() == nil {
		return "", false, false
	}
	pkgPath := pkg.PkgPath
	if pkg.Types != nil && pkg.Types.Path() != "" {
		pkgPath = pkg.Types.Path()
	}
	if tn.Pkg().Path() != pkgPath {
		return "", false, false
	}
	return tn.Name(), isPointer, true
}

func wrapperIsErrorExpr(expr ast.Expr, info *types.Info) bool {
	if info == nil {
		return false
	}
	tv, ok := info.Types[expr]
	if !ok {
		if ident, ok := expr.(*ast.Ident); ok {
			return ident.Name == "error"
		}
		return false
	}
	return tv.Type == types.Universe.Lookup("error").Type()
}

func configuredRuntimeValuesForFunc(fn *ast.FuncDecl, pkg *packages.Package) map[string]config.GoRuntimeValueConfig {
	if fn == nil || pkg == nil || pkg.Fset == nil {
		return nil
	}
	sourceFile := pkg.Fset.Position(fn.Pos()).Filename
	if sourceFile == "" {
		return nil
	}
	file, err := config.Load(sourceFile)
	if err != nil {
		return nil
	}
	return file.GoRuntimeValues
}

func configuredRuntimeValue(typeName string, configured map[string]config.GoRuntimeValueConfig, pkgName string) (config.GoRuntimeValueConfig, bool) {
	if len(configured) == 0 {
		return config.GoRuntimeValueConfig{}, false
	}
	trimmed := strings.TrimSpace(typeName)
	rv, ok := configured[trimmed]
	if !ok && pkgName != "" && !strings.Contains(trimmed, ".") && !strings.HasPrefix(trimmed, "*") {
		rv, ok = configured[pkgName+"."+trimmed]
	}
	if !ok || strings.TrimSpace(rv.Expression) == "" {
		return config.GoRuntimeValueConfig{}, false
	}
	return rv, true
}

// isFuncTypeSpelling reports whether goType is a Go function-type spelling
// (e.g. "func()", "func(string) error", "func(int) (T, error)"). Pointer-
// to-func (`*func(...)`) is intentionally excluded — that's a pointer
// parameter and goes through the nullable path.
//
// Scope: this matches raw func-literal spellings only. Named func types
// (e.g. `type Handler func(string) error` referenced as `Handler`,
// `http.HandlerFunc`) appear with their alias name in `goType` and are
// NOT caught here; they'd need either a runtimeval registry entry or
// type-checker-driven detection. The str-4cqz issue is scoped to raw
// `func(...)` shapes; named-func handling is deferred.
func isFuncTypeSpelling(goType string) bool {
	s := strings.TrimSpace(goType)
	return strings.HasPrefix(s, "func(") || strings.HasPrefix(s, "func ")
}

func extractWrapperParams(fn *ast.FuncDecl, info *types.Info, pkgName, pkgPath string, importSet map[string]struct{}) []WrapperParam {
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
		elemType := wrapperGoType(fieldType, info, pkgName, pkgPath, importSet)
		goType := elemType
		if isVariadic {
			goType = "[]" + elemType
		}
		needsMapInputNormalization := wrapperExprTypeContainsMap(fieldType, info)
		needsTimeInputNormalization := wrapperExprTypeContainsTime(fieldType, info)
		needsFuncInputNormalization := wrapperExprTypeContainsFunc(fieldType, info)
		needsRuntimeValueInputNormalization := wrapperExprTypeContainsRuntimeValue(fieldType, info)
		if len(field.Names) == 0 {
			// Unnamed parameter (e.g. `func F(int, string)`): a single
			// field with no names represents one positional parameter.
			params = append(params, WrapperParam{
				Name:                                syntheticParamName(index),
				GoType:                              goType,
				IsVariadic:                          isVariadic,
				NeedsMapInputNormalization:          needsMapInputNormalization,
				NeedsTimeInputNormalization:         needsTimeInputNormalization,
				NeedsFuncInputNormalization:         needsFuncInputNormalization,
				NeedsRuntimeValueInputNormalization: needsRuntimeValueInputNormalization,
			})
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
			params = append(params, WrapperParam{
				Name:                                localName,
				GoType:                              goType,
				IsVariadic:                          isVariadic,
				NeedsMapInputNormalization:          needsMapInputNormalization,
				NeedsTimeInputNormalization:         needsTimeInputNormalization,
				NeedsFuncInputNormalization:         needsFuncInputNormalization,
				NeedsRuntimeValueInputNormalization: needsRuntimeValueInputNormalization,
			})
			index++
		}
	}
	return params
}

func wrapperExprTypeContainsMap(expr ast.Expr, info *types.Info) bool {
	if expr == nil || info == nil {
		return false
	}
	tv, ok := info.Types[expr]
	if !ok || tv.Type == nil {
		return false
	}
	return wrapperTypeContainsMap(tv.Type, make(map[types.Type]struct{}))
}

func wrapperTypeContainsMap(typ types.Type, seen map[types.Type]struct{}) bool {
	if typ == nil {
		return false
	}
	if _, ok := seen[typ]; ok {
		return false
	}
	seen[typ] = struct{}{}

	switch t := typ.(type) {
	case *types.Map:
		return true
	case *types.Pointer:
		return wrapperTypeContainsMap(t.Elem(), seen)
	case *types.Slice:
		return wrapperTypeContainsMap(t.Elem(), seen)
	case *types.Array:
		return wrapperTypeContainsMap(t.Elem(), seen)
	case *types.Named:
		return wrapperTypeContainsMap(t.Underlying(), seen)
	case *types.Struct:
		for i := 0; i < t.NumFields(); i++ {
			if wrapperTypeContainsMap(t.Field(i).Type(), seen) {
				return true
			}
		}
	}
	return false
}

func wrapperExprTypeContainsTime(expr ast.Expr, info *types.Info) bool {
	if expr == nil || info == nil {
		return false
	}
	tv, ok := info.Types[expr]
	if !ok || tv.Type == nil {
		return false
	}
	return wrapperTypeContainsTime(tv.Type, make(map[types.Type]struct{}))
}

func wrapperTypeContainsTime(typ types.Type, seen map[types.Type]struct{}) bool {
	if typ == nil {
		return false
	}
	if _, ok := seen[typ]; ok {
		return false
	}
	seen[typ] = struct{}{}

	switch t := typ.(type) {
	case *types.Pointer:
		return wrapperTypeContainsTime(t.Elem(), seen)
	case *types.Slice:
		return wrapperTypeContainsTime(t.Elem(), seen)
	case *types.Array:
		return wrapperTypeContainsTime(t.Elem(), seen)
	case *types.Map:
		return wrapperTypeContainsTime(t.Key(), seen) || wrapperTypeContainsTime(t.Elem(), seen)
	case *types.Named:
		if wrapperNamedTypeIsTime(t) {
			return true
		}
		return wrapperTypeContainsTime(t.Underlying(), seen)
	case *types.Struct:
		for i := 0; i < t.NumFields(); i++ {
			if wrapperTypeContainsTime(t.Field(i).Type(), seen) {
				return true
			}
		}
	}
	return false
}

func wrapperExprTypeContainsFunc(expr ast.Expr, info *types.Info) bool {
	if expr == nil || info == nil {
		return false
	}
	tv, ok := info.Types[expr]
	if !ok || tv.Type == nil {
		return false
	}
	return wrapperTypeContainsFunc(tv.Type, make(map[types.Type]struct{}))
}

func wrapperTypeContainsFunc(typ types.Type, seen map[types.Type]struct{}) bool {
	if typ == nil {
		return false
	}
	if _, ok := seen[typ]; ok {
		return false
	}
	seen[typ] = struct{}{}

	switch t := typ.(type) {
	case *types.Signature:
		return true
	case *types.Pointer:
		return wrapperTypeContainsFunc(t.Elem(), seen)
	case *types.Slice:
		return wrapperTypeContainsFunc(t.Elem(), seen)
	case *types.Array:
		return wrapperTypeContainsFunc(t.Elem(), seen)
	case *types.Map:
		return wrapperTypeContainsFunc(t.Elem(), seen)
	case *types.Named:
		return wrapperTypeContainsFunc(t.Underlying(), seen)
	case *types.Struct:
		for i := 0; i < t.NumFields(); i++ {
			if wrapperTypeContainsFunc(t.Field(i).Type(), seen) {
				return true
			}
		}
	}
	return false
}

func wrapperExprTypeContainsRuntimeValue(expr ast.Expr, info *types.Info) bool {
	if expr == nil || info == nil {
		return false
	}
	tv, ok := info.Types[expr]
	if !ok || tv.Type == nil {
		return false
	}
	return wrapperTypeContainsRuntimeValue(tv.Type, make(map[types.Type]struct{}))
}

func wrapperTypeContainsRuntimeValue(typ types.Type, seen map[types.Type]struct{}) bool {
	if typ == nil {
		return false
	}
	if _, ok := seen[typ]; ok {
		return false
	}
	seen[typ] = struct{}{}

	switch t := typ.(type) {
	case *types.Pointer:
		return wrapperTypeContainsRuntimeValue(t.Elem(), seen)
	case *types.Slice:
		return wrapperTypeContainsRuntimeValue(t.Elem(), seen)
	case *types.Array:
		return wrapperTypeContainsRuntimeValue(t.Elem(), seen)
	case *types.Map:
		return wrapperTypeContainsRuntimeValue(t.Elem(), seen)
	case *types.Named:
		if wrapperNamedTypeIsWazeroCompiledModule(t) {
			return true
		}
		return wrapperTypeContainsRuntimeValue(t.Underlying(), seen)
	case *types.Struct:
		for i := 0; i < t.NumFields(); i++ {
			if wrapperTypeContainsRuntimeValue(t.Field(i).Type(), seen) {
				return true
			}
		}
	}
	return false
}

func wrapperNamedTypeIsTime(named *types.Named) bool {
	if named == nil || named.Obj() == nil || named.Obj().Pkg() == nil {
		return false
	}
	return named.Obj().Pkg().Path() == "time" && named.Obj().Name() == "Time"
}

func wrapperNamedTypeIsWazeroCompiledModule(named *types.Named) bool {
	if named == nil || named.Obj() == nil || named.Obj().Pkg() == nil {
		return false
	}
	return named.Obj().Pkg().Path() == "github.com/tetratelabs/wazero" && named.Obj().Name() == "CompiledModule"
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
//
// str-jeen.79: the qualifier compares p.Path() == pkgPath (the current
// package's full import path) instead of p.Name() == pkgName. Comparing
// by name alone incorrectly drops the qualifier for sibling packages that
// share the current package's short name (e.g. both named "mcp"), producing
// `undefined: Server` because the type `*mcp.Server` is emitted as `*Server`
// with no import. pkgName is retained for cases where pkgPath is unavailable
// (empty) to preserve backward-compatible behavior in test helpers that only
// supply the package name.
func wrapperGoType(expr ast.Expr, info *types.Info, pkgName, pkgPath string, importSet map[string]struct{}) string {
	if info != nil {
		if tv, ok := info.Types[expr]; ok && tv.Type != nil {
			qualifier := func(p *types.Package) string {
				if p == nil {
					return ""
				}
				// str-jeen.79: compare by full import path when available to
				// avoid stripping the qualifier for same-name sibling packages.
				if pkgPath != "" {
					if p.Path() == pkgPath {
						return ""
					}
				} else if p.Name() == pkgName {
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
func collectExtraImports(targets []WrapperTarget, constructors []ConstructorCandidate) []string {
	// str-jeen.73: only exclude the always-emitted core imports (encoding/json
	// and fmt). "strings" is NOT excluded here — it is conditionally emitted
	// by GenerateWrapper when either generic targets require it OR when target
	// parameter types reference the strings package (e.g. io.Reader runtime
	// value uses strings.NewReader). GenerateWrapper merges the two sources and
	// emits "strings" exactly once.
	const (
		coreImportJSON = "encoding/json"
		coreImportFmt  = "fmt"
	)
	seen := make(map[string]struct{})
	for _, t := range targets {
		for _, importPath := range t.Imports {
			trimmed := strings.TrimSpace(importPath)
			if trimmed == "" {
				continue
			}
			switch trimmed {
			case coreImportJSON, coreImportFmt:
				continue
			}
			seen[trimmed] = struct{}{}
		}
	}
	for _, c := range constructors {
		for _, p := range c.Parameters {
			for _, importPath := range p.Imports {
				trimmed := strings.TrimSpace(importPath)
				if trimmed == "" {
					continue
				}
				switch trimmed {
				case coreImportJSON, coreImportFmt:
					continue
				}
				seen[trimmed] = struct{}{}
			}
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
