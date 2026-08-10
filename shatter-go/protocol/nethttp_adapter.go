package protocol

import (
	"fmt"
)

// httpHandlerHook implements InvocationHook for net/http handler functions.
// It compiles and runs a specialized harness that invokes the handler with
// httptest infrastructure, capturing the HTTP response as the return value.
type httpHandlerHook struct{}

func (h *httpHandlerHook) ID() string { return HTTPHandlerAdapterID }

func (h *httpHandlerHook) Invoke(ctx InvocationContext) (*AdapterInvocationOutcome, error) {
	result, err := executeAdapterViaLauncher(HTTPHandlerAdapterID, ctx)
	if err != nil {
		return nil, fmt.Errorf("http handler execution: %w", err)
	}

	return &AdapterInvocationOutcome{
		Status:        OutcomeStatusCompleted,
		ReturnValue:   result.ReturnValue,
		ThrownError:   result.ThrownError,
		SideEffects:   result.SideEffects,
		BranchPath:    result.BranchPath,
		LinesExecuted: result.LinesExecuted,
		ScopeEvents:   result.ScopeEvents,
		ExternalCalls: result.ExternalCalls,
	}, nil
}

// httpHandlerFactory implements RuntimeHookFactory for the go/http-handler adapter.
type httpHandlerFactory struct{}

func (f *httpHandlerFactory) ID() string { return HTTPHandlerAdapterID }

func (f *httpHandlerFactory) CreateRuntimeHooks(_ ExecutionAdapter, _ RuntimeHookContext) *RuntimeHooks {
	return &RuntimeHooks{
		InvocationHooks: []InvocationHook{&httpHandlerHook{}},
	}
}

// createHTTPHandlerFactory returns a RuntimeHookFactory that creates an
// InvocationHook for net/http handler functions.
func createHTTPHandlerFactory() RuntimeHookFactory {
	return &httpHandlerFactory{}
}

// httpHandlerSyntheticParams returns the synthetic parameter definitions for
// the HTTP handler adapter. These replace the handler's real params
// (http.ResponseWriter, *http.Request) with HTTP request attributes that the
// explorer can generate.
func httpHandlerSyntheticParams() []ParamInfo {
	return []ParamInfo{
		{Name: "method", Type: TypeInfo{Kind: "str", Label: "string"}},
		{Name: "path", Type: TypeInfo{Kind: "str", Label: "string"}},
		{Name: "headers", Type: TypeInfo{Kind: "object", Fields: []ObjectField{}}},
		{Name: "body", Type: TypeInfo{Kind: "str", Label: "string"}},
	}
}
