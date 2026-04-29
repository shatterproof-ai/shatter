package protocol

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"sync"

	"golang.org/x/tools/go/packages"

	"github.com/shatter-dev/shatter/shatter-go/build"
	"github.com/shatter-dev/shatter/shatter-go/instrument"
	"github.com/shatter-dev/shatter/shatter-go/launcher"
	goloader "github.com/shatter-dev/shatter/shatter-go/loader"
	frontendtiming "github.com/shatter-dev/shatter/shatter-go/timing"
	"github.com/shatter-dev/shatter/shatter-go/workspace"
	"github.com/shatter-dev/shatter/shatter-go/wrapper"
)

type preparedExecution interface {
	IsValid() bool
	Cleanup()
	KillProc()
	// Invoke runs the prepared target with the implementation's default
	// receiver_kind (free-function path: "" baked in at prepare time).
	Invoke(inputs []json.RawMessage, capture bool) (*instrument.ExecuteResult, error)
	// InvokeWithReceiverKind dispatches a single invocation overriding the
	// default receiver_kind. Used by the receiver-aware Execute path
	// (str-hy9b.H5) to thread an InvocationPlan's receiver_kind into the
	// wrapper's switch without rebuilding the launcher binary. The empty
	// string means "use the default" (equivalent to plain Invoke).
	InvokeWithReceiverKind(receiverKind string, inputs []json.RawMessage, capture bool) (*instrument.ExecuteResult, error)
}

type preparedLauncher struct {
	ArtifactDir string
	BinaryPath  string
	// TargetID is the stable target identifier the wrapper's switch keys on
	// (`<pkg.PkgPath>:<qualified_name>`). Stored separately from the
	// default receiver_kind so receiver overrides don't risk picking up a
	// stale or differently-shaped target_id from a hand-crafted plan.
	TargetID string
	// DefaultReceiverKind is the receiver_kind used by Invoke and by
	// InvokeWithReceiverKind when the override is empty. The handler sets
	// this at prepare time; for free functions it's "", for method-aware
	// callers it can be e.g. "constructor:New".
	DefaultReceiverKind string
	// DefaultGenericTypeArgs is the generic_type_args list used by Invoke and
	// by InvokeWithPlan when the override is empty.
	DefaultGenericTypeArgs []string
	DiscDeps               []instrument.DiscoveredDependency

	mu      sync.Mutex
	session *launcher.LauncherSession
}

func (p *preparedLauncher) IsValid() bool {
	if p.ArtifactDir != "" {
		if _, err := os.Stat(p.ArtifactDir); err != nil {
			return false
		}
	}
	if _, err := os.Stat(p.BinaryPath); err != nil {
		return false
	}
	return true
}

func (p *preparedLauncher) Cleanup() {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.closeSessionLocked()
	if p.ArtifactDir != "" {
		_ = os.RemoveAll(p.ArtifactDir)
	}
}

func (p *preparedLauncher) KillProc() {
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.session == nil {
		return
	}
	_ = p.session.Kill()
	p.session = nil
}

// Invoke runs the prepared target with its default receiver_kind (set at
// prepare time). For free-function targets that's "", which the wrapper
// short-circuits to a direct call; for method-aware callers preferring
// non-default receiver strategies, use InvokeWithReceiverKind instead.
func (p *preparedLauncher) Invoke(inputs []json.RawMessage, capture bool) (*instrument.ExecuteResult, error) {
	return p.InvokeWithReceiverKind("", inputs, capture)
}

// InvokeWithReceiverKind dispatches a single launcher invocation, overriding
// the prepared target's DefaultReceiverKind when receiverKind is non-empty.
// The launcher binary itself does not depend on receiver_kind (the wrapper
// handles dispatch via PlanDescriptor.ReceiverKind), so a single prepared
// binary can serve invocations across multiple receiver strategies — no
// rebuild or cache invalidation is required when the override varies.
//
// The plan's TargetID is always taken from the prepared launcher's TargetID
// (the wrapper's source of truth) regardless of caller input. Callers that
// want to invoke a different target must build a separate prepared
// launcher; mismatched target_ids would otherwise hit the wrapper's
// "shatter: unknown target" error path.
func (p *preparedLauncher) InvokeWithReceiverKind(receiverKind string, inputs []json.RawMessage, capture bool) (*instrument.ExecuteResult, error) {
	return p.InvokeWithPlan(receiverKind, nil, inputs, capture)
}

func (p *preparedLauncher) InvokeWithPlan(receiverKind string, genericTypeArgs []string, inputs []json.RawMessage, capture bool) (*instrument.ExecuteResult, error) {
	rk := receiverKind
	if rk == "" {
		rk = p.DefaultReceiverKind
	}
	typeArgs := genericTypeArgs
	if len(typeArgs) == 0 && len(p.DefaultGenericTypeArgs) > 0 {
		typeArgs = p.DefaultGenericTypeArgs
	}
	planJSON, err := json.Marshal(InvocationPlan{
		TargetID:        p.TargetID,
		ReceiverKind:    rk,
		GenericTypeArgs: append([]string{}, typeArgs...),
		ArgumentPlans:   []ValuePlan{},
	})
	if err != nil {
		return nil, fmt.Errorf("marshal launcher plan: %w", err)
	}
	req := launcher.LauncherRequest{
		Plan:    planJSON,
		Inputs:  inputs,
		Capture: capture,
	}

	timeout := instrument.ExecTimeout()
	for attempt := 0; attempt < 2; attempt++ {
		session, err := p.sessionOrOpen()
		if err != nil {
			return nil, fmt.Errorf("launcher: open session: %w", err)
		}

		resp, err := session.InvokeWithTimeout(req, timeout)
		if err != nil {
			p.resetSession()
			if strings.Contains(err.Error(), "timed out") {
				return nil, err
			}
			if attempt == 0 {
				continue
			}
			return nil, err
		}
		if resp.Error != "" {
			return nil, errors.New(resp.Error)
		}
		return launcherResponseToExecuteResult(resp, p.DiscDeps)
	}

	return nil, fmt.Errorf("launcher: exhausted session retries")
}

func (p *preparedLauncher) sessionOrOpen() (*launcher.LauncherSession, error) {
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.session != nil {
		return p.session, nil
	}

	session, err := launcher.OpenSession(p.BinaryPath)
	if err != nil {
		return nil, err
	}
	p.session = session
	return session, nil
}

func (p *preparedLauncher) resetSession() {
	p.mu.Lock()
	defer p.mu.Unlock()
	p.closeSessionLocked()
}

func (p *preparedLauncher) closeSessionLocked() {
	if p.session == nil {
		return
	}
	_ = p.session.Close()
	p.session = nil
}

// prepareDirectExecution builds a launcher binary and returns a
// preparedLauncher carrying the supplied default receiver_kind and
// generic_type_args. Callers that omit both defaults get the legacy
// free-function plan; plan-aware callers should pass the receiver and generic
// arguments they expect Invoke to dispatch with by default. Per-Invoke
// overrides via InvokeWithPlan still work regardless of the defaults because
// the launcher binary is receiver-kind and generic-type-arg agnostic.
func (h *Handler) prepareDirectExecution(
	file string,
	function string,
	mocks []instrument.MockConfig,
	timing *frontendtiming.Collector,
	phasePrefix string,
	defaultReceiverKind string,
	defaultGenericTypeArgs []string,
) (*preparedLauncher, error) {
	absoluteFilePath, err := filepath.Abs(file)
	if err != nil {
		return nil, fmt.Errorf("normalize file path: %w", err)
	}

	finishAnalyze := timing.Start(phasePrefix + ".analyze")
	ws, ldr, err := h.ensureExecutionLoader(absoluteFilePath)
	if err != nil {
		finishAnalyze()
		return nil, err
	}

	pkg, err := loadPackageForAnalysis(ldr, absoluteFilePath)
	if err != nil {
		finishAnalyze()
		return nil, fmt.Errorf("analyzing function: %w", err)
	}

	req, targetID, err := buildDirectExecutionRequest(pkg, absoluteFilePath, function, mocks)
	finishAnalyze()
	if err != nil {
		return nil, fmt.Errorf("analyzing function: %w", err)
	}

	// The builder currently owns overlay instrumentation internally; record a
	// dedicated timing phase so the direct execute contract still reports it.
	finishInstrument := timing.Start(phasePrefix + ".instrument")
	finishInstrument()

	finishBuild := timing.Start(phasePrefix + ".build")
	result, err := build.NewBuilder(ws).Build(context.Background(), req)
	finishBuild()
	if err != nil {
		return nil, fmt.Errorf("build failed: %w", err)
	}

	return &preparedLauncher{
		BinaryPath:             result.BinaryPath,
		TargetID:               targetID,
		DefaultReceiverKind:    defaultReceiverKind,
		DefaultGenericTypeArgs: append([]string{}, defaultGenericTypeArgs...),
		DiscDeps:               instrument.DiscoverDependencies(absoluteFilePath, mocks),
	}, nil
}

func (h *Handler) ensureWorkspace(file string) (*workspace.Workspace, error) {
	if h.workspace == nil {
		ws, err := resolveExecutionWorkspace(file)
		if err != nil {
			return nil, fmt.Errorf("initialize workspace: %w", err)
		}
		h.workspace = ws
		instrument.SetWorkspaceGoEnvProvider(ws.GoEnv)
	}
	if err := h.workspace.Ensure(); err != nil {
		return nil, fmt.Errorf("ensure workspace: %w", err)
	}
	return h.workspace, nil
}

func (h *Handler) ensureExecutionLoader(file string) (*workspace.Workspace, *goloader.Loader, error) {
	ws, err := h.ensureWorkspace(file)
	if err != nil {
		return nil, nil, err
	}
	if h.loader == nil {
		ldr, err := goloader.New(ws)
		if err != nil {
			return nil, nil, fmt.Errorf("construct analyzer loader: %w", err)
		}
		h.loader = ldr
	}
	return ws, h.loader, nil
}

func resolveExecutionWorkspace(file string) (*workspace.Workspace, error) {
	if shouldLoadAsPackage(file) {
		return workspace.Initialize(workspace.ResolveOptions{StartDir: filepath.Dir(file)})
	}

	root := filepath.Join(filepath.Dir(file), ".shatter-cache", "go-workspace")
	ws, err := workspace.Open(root)
	if err != nil {
		return nil, err
	}
	if err := ws.Ensure(); err != nil {
		return nil, err
	}
	return ws, nil
}

// buildDirectExecutionRequest constructs the BuildRequest + canonical
// target_id for a single Execute target. The launcher binary itself is
// receiver-kind-agnostic (the wrapper switches on PlanDescriptor.ReceiverKind
// at invocation time), so this helper deliberately does not bake the
// receiver_kind into its output — each Invoke / InvokeWithReceiverKind
// call produces a fresh PlanDescriptor with the per-call receiver_kind.
// This keeps the prepared binary cacheable across receiver strategies.
//
// The returned target_id is the wrapper's stable identifier
// (`<pkg.PkgPath>:<qualified_name>`); callers store it on the prepared
// launcher so every subsequent invocation hits the wrapper's correct
// switch case regardless of how the high-level caller named the target.
func buildDirectExecutionRequest(
	pkg *packages.Package,
	absoluteFilePath string,
	function string,
	mocks []instrument.MockConfig,
) (build.BuildRequest, string, error) {
	targets := wrapper.BuildWrapperTargets(pkg)
	target, err := selectDirectWrapperTarget(targets, function)
	if err != nil {
		return build.BuildRequest{}, "", err
	}

	packageDir, err := packageDirForBuild(pkg)
	if err != nil {
		return build.BuildRequest{}, "", err
	}
	modulePath, moduleDir, err := moduleInfoForBuild(pkg, packageDir)
	if err != nil {
		return build.BuildRequest{}, "", err
	}

	return build.BuildRequest{
		Targets:                targets,
		Constructors:           toWrapperConstructors(ScanConstructors(pkg)),
		PackageName:            pkg.Name,
		TargetModulePath:       modulePath,
		TargetModuleDir:        moduleDir,
		TargetImportPath:       packageImportPathForBuild(pkg, modulePath),
		TargetPackageDir:       packageDir,
		InstrumentedSourceFile: packageFileForBuild(pkg, absoluteFilePath),
		Mocks:                  mocks,
	}, target.ID, nil
}

// selectDirectWrapperTarget returns the wrapper target matching `function`.
// Method targets (str-hy9b.H5) are now legal first-class targets; selection
// prefers a free function when both share a name, matching pre-H5 behavior
// for the free-function path.
func selectDirectWrapperTarget(targets []wrapper.WrapperTarget, function string) (wrapper.WrapperTarget, error) {
	var methodMatch *wrapper.WrapperTarget
	for i, target := range targets {
		if target.SymbolName != function {
			continue
		}
		if target.Kind == wrapper.TargetKindFunction {
			return target, nil
		}
		if target.Kind == wrapper.TargetKindMethod && methodMatch == nil {
			methodMatch = &targets[i]
		}
	}
	if methodMatch != nil {
		return *methodMatch, nil
	}
	return wrapper.WrapperTarget{}, fmt.Errorf("function not found: %s", function)
}

func toWrapperConstructors(candidates []ConstructorCandidate) []wrapper.ConstructorCandidate {
	if len(candidates) == 0 {
		return nil
	}
	constructors := make([]wrapper.ConstructorCandidate, len(candidates))
	for i, candidate := range candidates {
		constructors[i] = wrapper.ConstructorCandidate{
			FuncName:   candidate.FuncName,
			TargetType: candidate.TargetType,
		}
	}
	return constructors
}

func packageDirForBuild(pkg *packages.Package) (string, error) {
	files := pkg.GoFiles
	if len(files) == 0 {
		files = pkg.CompiledGoFiles
	}
	if len(files) == 0 {
		return "", fmt.Errorf("package has no Go files")
	}
	return filepath.Dir(files[0]), nil
}

func moduleInfoForBuild(pkg *packages.Package, packageDir string) (modulePath string, moduleDir string, err error) {
	if pkg.Module != nil && pkg.Module.Path != "" && pkg.Module.Dir != "" {
		return pkg.Module.Path, pkg.Module.Dir, nil
	}

	moduleDir, found := findGoModuleRoot(packageDir)
	if !found {
		return "", "", fmt.Errorf("module root not found for %s", packageDir)
	}
	if pkg.PkgPath == "" {
		return "", "", fmt.Errorf("package import path missing for %s", packageDir)
	}

	rel, relErr := filepath.Rel(moduleDir, packageDir)
	if relErr == nil && rel != "." {
		suffix := filepath.ToSlash(rel)
		if strings.HasSuffix(pkg.PkgPath, "/"+suffix) {
			return strings.TrimSuffix(pkg.PkgPath, "/"+suffix), moduleDir, nil
		}
	}
	return pkg.PkgPath, moduleDir, nil
}

func packageImportPathForBuild(pkg *packages.Package, modulePath string) string {
	if pkg.PkgPath != "" {
		return pkg.PkgPath
	}
	return modulePath
}

func packageFileForBuild(pkg *packages.Package, absoluteFilePath string) string {
	candidates := append([]string{}, pkg.GoFiles...)
	candidates = append(candidates, pkg.CompiledGoFiles...)
	for _, candidate := range candidates {
		if sameSourceFile(candidate, absoluteFilePath) {
			return candidate
		}
	}
	if len(candidates) == 1 {
		return candidates[0]
	}
	return absoluteFilePath
}

func sameSourceFile(candidate string, absoluteFilePath string) bool {
	candidateAbs, err := filepath.Abs(candidate)
	if err != nil {
		candidateAbs = candidate
	}
	return candidateAbs == absoluteFilePath
}

func launcherResponseToExecuteResult(
	resp launcher.LauncherResponse,
	deps []instrument.DiscoveredDependency,
) (*instrument.ExecuteResult, error) {
	branchPath, err := decodeJSONArray[instrument.BranchDecision](resp.BranchPath)
	if err != nil {
		return nil, fmt.Errorf("decode branch_path: %w", err)
	}
	linesExecuted, err := decodeJSONArray[int](resp.LinesExecuted)
	if err != nil {
		return nil, fmt.Errorf("decode lines_executed: %w", err)
	}
	scopeEvents, err := decodeJSONArray[json.RawMessage](resp.ScopeEvents)
	if err != nil {
		return nil, fmt.Errorf("decode scope_events: %w", err)
	}
	externalCalls, err := decodeJSONArray[instrument.ExternalCall](resp.ExternalCalls)
	if err != nil {
		return nil, fmt.Errorf("decode external_calls: %w", err)
	}

	result := &instrument.ExecuteResult{
		ReturnValue:            resp.ReturnValue,
		ThrownError:            convertLauncherError(resp.ThrownError),
		BranchPath:             branchPath,
		LinesExecuted:          linesExecuted,
		ExternalCalls:          externalCalls,
		DiscoveredDependencies: deps,
		SideEffects:            convertLauncherSideEffects(resp.SideEffects),
		ScopeEvents:            scopeEvents,
		Performance:            convertLauncherPerf(resp.Performance),
	}
	return result, nil
}

func decodeJSONArray[T any](data json.RawMessage) ([]T, error) {
	if len(data) == 0 {
		return []T{}, nil
	}
	var decoded []T
	if err := json.Unmarshal(data, &decoded); err != nil {
		return nil, err
	}
	if decoded == nil {
		return []T{}, nil
	}
	return decoded, nil
}

func convertLauncherSideEffects(effects []launcher.LauncherSideEffect) []instrument.SideEffect {
	if len(effects) == 0 {
		return []instrument.SideEffect{}
	}
	converted := make([]instrument.SideEffect, len(effects))
	for i, effect := range effects {
		before := effect.Before
		after := effect.After
		converted[i] = instrument.SideEffect{
			Kind:     effect.Kind,
			Level:    effect.Level,
			Message:  effect.Message,
			Variable: effect.Variable,
			Before:   &before,
			After:    &after,
		}
		if len(effect.Before) == 0 {
			converted[i].Before = nil
		}
		if len(effect.After) == 0 {
			converted[i].After = nil
		}
	}
	return converted
}

func convertLauncherError(err *launcher.LauncherError) *instrument.ErrorInfo {
	if err == nil {
		return nil
	}
	var category *string
	if err.ErrorCategory != "" {
		category = &err.ErrorCategory
	}
	return &instrument.ErrorInfo{
		ErrorType:     err.ErrorType,
		Message:       err.Message,
		Stack:         err.Stack,
		ErrorCategory: category,
	}
}

func convertLauncherPerf(perf *launcher.LauncherPerf) instrument.PerfMetrics {
	if perf == nil {
		return instrument.PerfMetrics{}
	}
	return instrument.PerfMetrics{
		WallTimeMs:         perf.WallTimeMs,
		CPUTimeUs:          int(perf.CPUTimeUs),
		HeapUsedBytes:      int(perf.HeapUsedBytes),
		HeapAllocatedBytes: int(perf.HeapAllocatedBytes),
	}
}
