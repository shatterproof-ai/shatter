package protocol

import (
	"encoding/json"
	"fmt"

	"github.com/shatter-dev/shatter/shatter-go/instrument"
)

// InvocationContext is the context passed to an InvocationHook.
type InvocationContext struct {
	File            string
	FunctionName    string
	InvocationModel *InvocationModel
	Inputs          []json.RawMessage
	Capture         bool
}

// InvocationOutcome is the structured result from adapter-owned invocation.
// Fields ride back through existing ExecuteResponse fields — no new wire types.
// SideEffects uses instrument.SideEffect to avoid a redundant conversion step;
// the handler's convertSideEffects maps instrument→protocol on the response path.
type InvocationOutcome struct {
	ReturnValue json.RawMessage
	ThrownError *instrument.ErrorInfo
	SideEffects []instrument.SideEffect
}

// InvocationHook dispatches adapter-owned invocation for a specific adapter ID.
// Resolved by ID, which must equal InvocationModel.AdapterID.
type InvocationHook interface {
	ID() string
	Invoke(ctx InvocationContext) (*InvocationOutcome, error)
}

// RuntimeHookContext carries request-scoped metadata for factory resolution.
type RuntimeHookContext struct {
	Phase        string // "execute" or "setup"
	ProjectRoot  string
	EntryFile    string
	FunctionName string
}

// RuntimeHooks aggregates all hooks resolved from an ExecutionProfile.
// Go's subprocess model does not need resolver_adapters or sandbox_providers;
// only invocation hooks are relevant.
type RuntimeHooks struct {
	InvocationHooks []InvocationHook
}

// RuntimeHookFactory creates RuntimeHooks for a specific adapter ID.
type RuntimeHookFactory interface {
	ID() string
	CreateRuntimeHooks(adapter ExecutionAdapter, ctx RuntimeHookContext) *RuntimeHooks
}

// InvocationStrategy is the result of ChooseInvocationStrategy.
type InvocationStrategy struct {
	Kind      string           // "direct", "adapter", or "unsupported"
	Hook      InvocationHook   // non-nil only for "adapter"
	Model     *InvocationModel // non-nil only for "adapter"
	AdapterID string           // non-empty only for "unsupported"
}

// ChooseInvocationStrategy is a pure dispatcher that decides whether a given
// analysis routes to the direct path, an adapter-owned path, or an unsupported
// failure. Mirrors the TS chooseInvocationStrategy in runtime-hooks.ts.
func ChooseInvocationStrategy(model *InvocationModel, hooks []InvocationHook) InvocationStrategy {
	if model == nil || model.Kind == "direct" {
		return InvocationStrategy{Kind: "direct"}
	}
	for _, h := range hooks {
		if h.ID() == model.AdapterID {
			return InvocationStrategy{Kind: "adapter", Hook: h, Model: model}
		}
	}
	return InvocationStrategy{Kind: "unsupported", AdapterID: model.AdapterID}
}

// ResolveRuntimeHooks resolves an ExecutionProfile against a set of factories,
// producing merged RuntimeHooks. Adapters with apply=="disabled" are skipped.
// Returns an error if an adapter ID has no matching factory.
func ResolveRuntimeHooks(profile *ExecutionProfile, ctx RuntimeHookContext, factories []RuntimeHookFactory) (RuntimeHooks, error) {
	var merged RuntimeHooks
	if profile == nil || len(profile.Adapters) == 0 {
		return merged, nil
	}

	factoryByID := make(map[string]RuntimeHookFactory, len(factories))
	for _, f := range factories {
		factoryByID[f.ID()] = f
	}

	for _, adapter := range profile.Adapters {
		if adapter.Apply != nil && *adapter.Apply == ExecutionAdapterApplyDisabled {
			continue
		}
		factory, ok := factoryByID[adapter.ID]
		if !ok {
			return merged, fmt.Errorf("execution adapter not supported by Go frontend: %s", adapter.ID)
		}
		hooks := factory.CreateRuntimeHooks(adapter, ctx)
		if hooks != nil {
			merged.InvocationHooks = append(merged.InvocationHooks, hooks.InvocationHooks...)
		}
	}

	return merged, nil
}

// ExecuteAdapterOwned invokes a target through an adapter hook instead of the
// instrumented subprocess harness. Returns an instrument.ExecuteResult with
// empty instrumentation fields (branch_path, lines_executed, path_constraints,
// calls_to_external) since adapter-owned calls are not instrumented.
func ExecuteAdapterOwned(hook InvocationHook, ctx InvocationContext) (*instrument.ExecuteResult, error) {
	outcome, err := hook.Invoke(ctx)
	if err != nil {
		return nil, fmt.Errorf("adapter %s invoke failed: %w", hook.ID(), err)
	}

	sideEffects := outcome.SideEffects
	if sideEffects == nil {
		sideEffects = []instrument.SideEffect{}
	}

	result := &instrument.ExecuteResult{
		ReturnValue:            outcome.ReturnValue,
		ThrownError:            outcome.ThrownError,
		BranchPath:             []instrument.BranchDecision{},
		LinesExecuted:          []int{},
		ExternalCalls:          []instrument.ExternalCall{},
		DiscoveredDependencies: []instrument.DiscoveredDependency{},
		SideEffects:            sideEffects,
		ScopeEvents:            []json.RawMessage{},
	}

	return result, nil
}
