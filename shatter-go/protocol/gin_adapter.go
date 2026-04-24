package protocol

import (
	"fmt"
)

// ginHandlerHook implements InvocationHook for Gin handler functions.
// It compiles and runs a specialized harness that invokes the handler with
// a synthetic gin.Context, capturing the HTTP response as the return value.
type ginHandlerHook struct{}

func (h *ginHandlerHook) ID() string { return GinAdapterID }

func (h *ginHandlerHook) Invoke(ctx InvocationContext) (*AdapterInvocationOutcome, error) {
	result, err := executeAdapterViaLauncher(GinAdapterID, ctx)
	if err != nil {
		return nil, fmt.Errorf("gin handler execution: %w", err)
	}

	return &AdapterInvocationOutcome{
		Status:      OutcomeStatusCompleted,
		ReturnValue: result.ReturnValue,
		ThrownError: result.ThrownError,
		SideEffects: result.SideEffects,
	}, nil
}

// ginHandlerFactory implements RuntimeHookFactory for the go/gin adapter.
type ginHandlerFactory struct{}

func (f *ginHandlerFactory) ID() string { return GinAdapterID }

func (f *ginHandlerFactory) CreateRuntimeHooks(_ ExecutionAdapter, _ RuntimeHookContext) *RuntimeHooks {
	return &RuntimeHooks{
		InvocationHooks: []InvocationHook{&ginHandlerHook{}},
	}
}

// createGinHandlerFactory returns a RuntimeHookFactory that creates an
// InvocationHook for Gin handler functions.
func createGinHandlerFactory() RuntimeHookFactory {
	return &ginHandlerFactory{}
}

// ginHandlerSyntheticParams returns the synthetic parameter definitions for
// the Gin handler adapter. These replace the handler's real param
// (*gin.Context) with HTTP request attributes that the explorer can generate.
func ginHandlerSyntheticParams() []ParamInfo {
	return []ParamInfo{
		{Name: "method", Type: TypeInfo{Kind: "primitive", Label: "string"}},
		{Name: "path", Type: TypeInfo{Kind: "primitive", Label: "string"}},
		{Name: "headers", Type: TypeInfo{Kind: "object", Fields: []ObjectField{}}},
		{Name: "body", Type: TypeInfo{Kind: "primitive", Label: "string"}},
		{Name: "route_params", Type: TypeInfo{Kind: "object", Fields: []ObjectField{}}},
	}
}
