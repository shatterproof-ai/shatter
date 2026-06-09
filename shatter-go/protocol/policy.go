package protocol

// Default safety policy for Go execution targets (str-hy9b.G4).
//
// A SideEffectClass is a coarse pre-execution category used by the policy
// gate to decide whether a plan is safe to run. It is distinct from the
// seven wire-format side-effect kinds in protocol/schemas/side-effect.schema.json,
// which describe effects *observed* during an execution. Classes are
// predicted from static FunctionAnalysis data (parameter types + declared
// external dependencies) before any harness is launched.
//
// Default allow set: ClassPure, ClassLocalFS.
// Default deny set:  ClassNetwork, ClassSubprocess, ClassDatabase,
//                    ClassProcessGlobal, ClassUnknownHigh.
//
// Local_fs sandbox enforcement (confining file I/O to
// <workspace>/runs/<runID>/sandbox/) is deferred to a separate story;
// G4 treats local_fs as allow-by-default unconditionally.

import (
	"context"
	"fmt"
	"path/filepath"
	"strings"

	"github.com/shatter-dev/shatter/shatter-go/config"
)

type SideEffectClass string

const (
	ClassPure          SideEffectClass = "pure"
	ClassLocalFS       SideEffectClass = "local_fs"
	ClassNetwork       SideEffectClass = "network"
	ClassSubprocess    SideEffectClass = "subprocess"
	ClassDatabase      SideEffectClass = "database"
	ClassProcessGlobal SideEffectClass = "process_global"
	ClassUnknownHigh   SideEffectClass = "unknown_high"
)

// knownClasses lists every recognized SideEffectClass. Config allow entries
// outside this set are silently ignored with a log warning.
var knownClasses = map[SideEffectClass]bool{
	ClassPure:          true,
	ClassLocalFS:       true,
	ClassNetwork:       true,
	ClassSubprocess:    true,
	ClassDatabase:      true,
	ClassProcessGlobal: true,
	ClassUnknownHigh:   true,
}

// ClassifiedUse records one attribution of a side-effect class to a
// concrete component within a target function. Evidence is the specific
// symbol or parameter type that triggered the classification.
type ClassifiedUse struct {
	Class     SideEffectClass
	Component string
	Evidence  string
}

// PolicyDecision is the result of evaluating a target against a policy.
type PolicyDecision struct {
	Allow     bool
	Reason    string
	Offending ClassifiedUse
}

// defaultAllowedClasses returns the baseline allow set every execution
// inherits regardless of config.
func defaultAllowedClasses() map[SideEffectClass]bool {
	return map[SideEffectClass]bool{
		ClassPure:    true,
		ClassLocalFS: true,
	}
}

// classifyFunction predicts the side-effect classes a function will touch
// based on its parameter types and declared external dependencies. It
// returns one ClassifiedUse per attributed component. An empty slice means
// the function is classified as pure.
func classifyFunction(fa *FunctionAnalysis) []ClassifiedUse {
	if fa == nil {
		return nil
	}
	var uses []ClassifiedUse
	for _, p := range fa.Params {
		if use, ok := classifyParam(p); ok {
			uses = append(uses, use)
		}
	}
	for _, d := range fa.Dependencies {
		if use, ok := classifyDependency(d); ok {
			uses = append(uses, use)
		}
	}
	if use, ok := classifyReturnType(fa.ReturnType); ok {
		uses = append(uses, use)
	}
	return uses
}

// classifyFunctionWithLocalDependencies expands policy classification through
// cached same-package helper calls. This catches wrappers such as New ->
// NewContext where only the helper directly touches a subprocess launcher.
func classifyFunctionWithLocalDependencies(
	fa *FunctionAnalysis,
	lookupLocal func(string) *FunctionAnalysis,
) []ClassifiedUse {
	if lookupLocal == nil {
		return classifyFunction(fa)
	}
	var uses []ClassifiedUse
	seen := map[*FunctionAnalysis]bool{}
	var walk func(*FunctionAnalysis)
	walk = func(current *FunctionAnalysis) {
		if current == nil {
			return
		}
		if seen[current] {
			return
		}
		seen[current] = true
		uses = append(uses, classifyFunction(current)...)
		for _, dep := range current.Dependencies {
			child := lookupLocal(dep.Symbol)
			if child != nil {
				walk(child)
			}
		}
	}
	walk(fa)
	return uses
}

// classifyParam inspects a single parameter for a dangerous opaque type.
// Detection works off TypeInfo.Label (e.g. "sql.DB") and the optional
// TypeName hint (e.g. "*sql.DB"). Returns ok=false for parameters that
// carry no side-effect signal.
func classifyParam(p ParamInfo) (ClassifiedUse, bool) {
	label := p.Type.Label
	typeName := ""
	if p.TypeName != nil {
		typeName = *p.TypeName
	}
	candidate := label
	if candidate == "" {
		candidate = typeName
	}
	if candidate == "" {
		return ClassifiedUse{}, false
	}
	if isSafeRuntimeValueParam(label, typeName) {
		return ClassifiedUse{}, false
	}
	class, ok := paramTypeClass(candidate)
	if !ok {
		return ClassifiedUse{}, false
	}
	component := candidate
	if typeName != "" {
		component = typeName
	}
	return ClassifiedUse{
		Class:     class,
		Component: component,
		Evidence:  fmt.Sprintf("param %q type %s", p.Name, component),
	}, true
}

func isSafeRuntimeValueParam(label, typeName string) bool {
	switch {
	case isHTTPResponseWriterType(label) || isHTTPResponseWriterType(typeName):
		return true
	case typeName == "*http.Request":
		return true
	default:
		return false
	}
}

func isHTTPResponseWriterType(typeName string) bool {
	return strings.TrimLeft(typeName, "*[]") == "http.ResponseWriter"
}

func classifyReturnType(t TypeInfo) (ClassifiedUse, bool) {
	if class, component, ok := typeInfoClass(t); ok {
		return ClassifiedUse{
			Class:     class,
			Component: component,
			Evidence:  fmt.Sprintf("return type %s", component),
		}, true
	}
	return ClassifiedUse{}, false
}

// paramTypeClass maps a parameter type label (e.g. "sql.DB") to the
// side-effect class implied by accepting that type. The map is
// intentionally small and fails open — unknown types return ok=false so
// the dependency walk can still classify them via their symbols.
func paramTypeClass(label string) (SideEffectClass, bool) {
	// Strip leading pointer and slice markers; detection works on the
	// qualified type identifier.
	norm := strings.TrimLeft(label, "*[]")
	switch norm {
	case "sql.DB", "sql.Tx", "sql.Conn", "sql.Stmt", "sql.Rows":
		return ClassDatabase, true
	case "net.Conn", "net.Listener", "http.Client", "http.Request", "http.ResponseWriter":
		return ClassNetwork, true
	case "Browser", "scraper.Browser", "rod.Browser", "launcher.Launcher":
		return ClassSubprocess, true
	}
	return "", false
}

func typeInfoClass(t TypeInfo) (SideEffectClass, string, bool) {
	if isHTTPResponseWriterType(t.Label) {
		return "", "", false
	}
	if t.Label != "" {
		if class, ok := paramTypeClass(t.Label); ok {
			return class, t.Label, true
		}
	}
	if t.Element != nil {
		if class, component, ok := typeInfoClass(*t.Element); ok {
			return class, component, true
		}
	}
	if t.Inner != nil {
		if class, component, ok := typeInfoClass(*t.Inner); ok {
			return class, component, true
		}
	}
	for _, field := range t.Fields {
		if class, component, ok := typeInfoClass(field.Type); ok {
			return class, component, true
		}
	}
	for _, variant := range t.Variants {
		if class, component, ok := typeInfoClass(variant); ok {
			return class, component, true
		}
	}
	return "", "", false
}

// classifyDependency maps an ExternalDependency to a side-effect class
// based on its declared source module and symbol.
func classifyDependency(d ExternalDependency) (ClassifiedUse, bool) {
	if isPureDependency(d) {
		return ClassifiedUse{}, false
	}
	class, ok := moduleClass(d.SourceModule, d.Symbol)
	if !ok {
		var component string
		class, component, ok = typeInfoClass(d.ReturnType)
		if !ok {
			return ClassifiedUse{}, false
		}
		return ClassifiedUse{
			Class:     class,
			Component: component,
			Evidence:  fmt.Sprintf("dependency %s return type %s", d.Symbol, component),
		}, true
	}
	component := d.Symbol
	if component == "" {
		component = d.SourceModule
	}
	return ClassifiedUse{
		Class:     class,
		Component: component,
		Evidence:  fmt.Sprintf("dependency %s (%s)", d.Symbol, d.SourceModule),
	}, true
}

func isPureDependency(d ExternalDependency) bool {
	switch d.SourceModule {
	case "net/http":
		return isPureNetHTTPSymbol(d.Symbol)
	case "net":
		return isPureNetSymbol(d.Symbol)
	default:
		return false
	}
}

// moduleClass classifies an external symbol by module path. The table is
// intentionally conservative: anything not recognized is treated as
// ClassUnknownHigh so the policy gate defaults to deny.
func moduleClass(module, symbol string) (SideEffectClass, bool) {
	if module == "" && symbol == "" {
		return "", false
	}
	switch {
	case module == "database/sql",
		strings.HasPrefix(module, "github.com/jmoiron/sqlx"),
		strings.HasPrefix(module, "gorm.io/gorm"):
		return ClassDatabase, true
	case module == "net/http":
		if isPureNetHTTPSymbol(symbol) {
			return "", false
		}
		return ClassNetwork, true
	case module == "net":
		if isPureNetSymbol(symbol) {
			return "", false
		}
		return ClassNetwork, true
	case strings.HasPrefix(module, "golang.org/x/net"):
		return ClassNetwork, true
	case module == "os/exec",
		module == "syscall",
		strings.HasPrefix(module, "github.com/go-rod/rod"),
		strings.HasPrefix(module, "github.com/go-rod/stealth"),
		strings.HasPrefix(module, "github.com/playwright-community/playwright-go"),
		strings.HasPrefix(module, "github.com/chromedp/chromedp"),
		strings.HasPrefix(module, "github.com/tebeka/selenium"):
		return ClassSubprocess, true
	case module == "os":
		osSymbol := unqualifyStdlibSymbol(module, symbol)
		if isPureOsSymbol(osSymbol) {
			return "", false
		}
		if isProcessGlobalOsSymbol(osSymbol) {
			return ClassProcessGlobal, true
		}
		if isLocalFSOsSymbol(osSymbol) {
			return ClassLocalFS, true
		}
		// os.* calls we don't recognize — fail closed.
		return ClassUnknownHigh, true
	case module == "io/ioutil":
		return ClassLocalFS, true
	case module == "path/filepath":
		// Walk / WalkDir touch the filesystem; other path/filepath symbols
		// are pure string manipulation.
		if symbol == "Walk" || symbol == "WalkDir" || symbol == "Glob" {
			return ClassLocalFS, true
		}
		return "", false
	case module == "crypto/rand":
		if isLocalRandomSymbol(symbol) {
			return "", false
		}
		return ClassUnknownHigh, true
	}
	return "", false
}

func isPureNetHTTPSymbol(symbol string) bool {
	switch strings.TrimPrefix(symbol, "http.") {
	case "Error", "HandlerFunc", "NewRequest", "NewRequestWithContext", "NotFound":
		return true
	default:
		return false
	}
}

func isPureNetSymbol(symbol string) bool {
	switch strings.TrimPrefix(symbol, "net.") {
	case "ParseIP", "SplitHostPort":
		return true
	default:
		return false
	}
}

func isLocalRandomSymbol(symbol string) bool {
	switch strings.TrimPrefix(symbol, "rand.") {
	case "Read":
		return true
	default:
		return false
	}
}

func unqualifyStdlibSymbol(module, symbol string) string {
	return strings.TrimPrefix(symbol, module+".")
}

func isPureOsSymbol(symbol string) bool {
	switch symbol {
	case "IsExist", "IsNotExist", "IsPermission", "IsTimeout":
		return true
	}
	return false
}

func isProcessGlobalOsSymbol(symbol string) bool {
	switch symbol {
	case "Setenv", "Unsetenv", "Clearenv", "Chdir", "Exit", "Setuid", "Setgid":
		return true
	}
	return false
}

func isLocalFSOsSymbol(symbol string) bool {
	switch symbol {
	case "Open", "OpenFile", "Create", "CreateTemp", "ReadFile", "WriteFile",
		"Remove", "RemoveAll", "Rename", "Mkdir", "MkdirAll", "MkdirTemp",
		"Stat", "Lstat", "Symlink", "Link", "Chmod", "Chown", "Truncate",
		"ReadDir", "ReadAll":
		return true
	}
	return false
}

// evaluatePolicy applies the decision rule against a pre-classified list
// of uses. The first use whose class is not in allowed produces a deny
// decision with a one-sentence ShortReason suitable for
// InvocationOutcome.ShortReason.
func evaluatePolicy(uses []ClassifiedUse, allowed map[SideEffectClass]bool) PolicyDecision {
	for _, u := range uses {
		if !allowed[u.Class] {
			return PolicyDecision{
				Allow:     false,
				Offending: u,
				Reason: fmt.Sprintf(
					"skipped: side effect class=%s (component=%s, evidence=%s) not in policy.allow",
					u.Class, u.Component, u.Evidence,
				),
			}
		}
	}
	return PolicyDecision{Allow: true}
}

// isAdapterOwned reports whether a function should bypass the policy
// gate because its execution runs inside an adapter's curated harness.
func isAdapterOwned(fa *FunctionAnalysis) bool {
	return fa != nil && fa.InvocationModel != nil && fa.InvocationModel.Kind == "adapter"
}

// evaluateExecutePolicy looks up per-target overrides, classifies the
// target, and returns the resulting decision. The second return value
// is false when the gate should not be applied (e.g. loader errored or
// analysis missing — fail open so unrelated bugs don't block execute).
func (h *Handler) evaluateExecutePolicy(file, function string, fa *FunctionAnalysis) (PolicyDecision, bool) {
	if fa == nil {
		return PolicyDecision{}, false
	}
	cfg, err := h.loadPolicyConfig(file)
	if err != nil {
		h.log.Log(context.Background(), LevelTrace, "policy config load failed; proceeding with defaults", "err", err, "file", file)
		cfg = config.File{}
	}
	relpath := policyTargetRelpath(file)
	entry := cfg.MatchTarget(relpath, function)
	var overrides []string
	if entry.Policy != nil {
		overrides = entry.Policy.Allow
	}
	logUnknown := func(raw string) {
		h.log.Warn("ignoring unknown policy.allow entry", "value", raw, "file", file, "function", function)
	}
	allowed := buildAllowedSet(overrides, logUnknown)
	uses := h.classifyFunctionForPolicy(file, fa)
	return evaluatePolicy(uses, allowed), true
}

func (h *Handler) classifyFunctionForPolicy(file string, fa *FunctionAnalysis) []ClassifiedUse {
	return classifyFunctionWithLocalDependencies(fa, func(symbol string) *FunctionAnalysis {
		if symbol == "" {
			return nil
		}
		return h.cachedAnalyses[file+"\x00"+symbol]
	})
}

// loadPolicyConfig invokes the injected loader or falls back to the
// real filesystem loader.
func (h *Handler) loadPolicyConfig(file string) (config.File, error) {
	if h.policyConfigLoader != nil {
		return h.policyConfigLoader(file)
	}
	return config.Load(file)
}

// policyTargetRelpath returns the path component used in config match
// keys. It is the file's basename if the path is absolute or escapes
// the working directory; otherwise the original path is preserved so
// nested patterns like "models/user.go:*" still work.
func policyTargetRelpath(file string) string {
	clean := filepath.ToSlash(filepath.Clean(file))
	if filepath.IsAbs(clean) {
		return filepath.Base(clean)
	}
	if strings.HasPrefix(clean, "../") {
		return filepath.Base(clean)
	}
	return clean
}

// buildAllowedSet constructs the final allowed-class set by unioning the
// default allow set with any per-target overrides supplied via config.
// Unknown allow strings are dropped (with a caller-visible log message)
// rather than failing the request.
func buildAllowedSet(overrides []string, logUnknown func(string)) map[SideEffectClass]bool {
	allowed := defaultAllowedClasses()
	for _, raw := range overrides {
		cls := SideEffectClass(strings.TrimSpace(raw))
		if !knownClasses[cls] {
			if logUnknown != nil {
				logUnknown(raw)
			}
			continue
		}
		allowed[cls] = true
	}
	return allowed
}
