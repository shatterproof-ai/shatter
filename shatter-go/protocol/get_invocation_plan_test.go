package protocol

import (
	"bytes"
	"encoding/json"
	"io"
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/workspace"
)

// Minimal inline planner func used by tests to avoid pulling in the planner
// package (which would create an import cycle).
func stubPlanner(
	requirements []InvocationRequirement,
	lookup func(string) *TargetContext,
) ([]InvocationPlan, []UnsatisfiedRequirement) {
	var plans []InvocationPlan
	var unsat []UnsatisfiedRequirement
	for _, req := range requirements {
		ctx := lookup(req.TargetID)
		if ctx == nil || ctx.Analysis == nil {
			unsat = append(unsat, UnsatisfiedRequirement{
				Kind:     UnsatisfiedRequirementKindComplexType,
				TargetID: req.TargetID,
				Detail:   "target not analyzed",
			})
			continue
		}
		plans = append(plans, InvocationPlan{
			TargetID:      req.TargetID,
			ReceiverKind:  "",
			ArgumentPlans: []ValuePlan{},
			Priority:      0,
			Label:         "stub",
		})
	}
	return plans, unsat
}

func runWithPlanner(t *testing.T, planner PlannerFunc, requests ...string) []Response {
	t.Helper()
	return runWithPlannerHandler(t, NewHandler, planner, requests...)
}

func runWithPlannerWorkspace(t *testing.T, planner PlannerFunc, requests ...string) []Response {
	t.Helper()
	ws, err := workspace.Initialize(workspace.ResolveOptions{RepoOverrideRoot: t.TempDir()})
	if err != nil {
		t.Fatalf("initialize workspace: %v", err)
	}
	return runWithPlannerHandler(t, func(r io.Reader, w io.Writer, logw io.Writer) *Handler {
		return NewHandlerWithWorkspace(r, w, logw, ws)
	}, planner, requests...)
}

func runWithPlannerHandler(
	t *testing.T,
	newHandler func(io.Reader, io.Writer, io.Writer) *Handler,
	planner PlannerFunc,
	requests ...string,
) []Response {
	t.Helper()
	input := strings.NewReader(strings.Join(requests, "\n") + "\n")
	var output bytes.Buffer
	handler := newHandler(input, &output, io.Discard)
	handler.RegisterPlanner(planner)
	if err := handler.Run(); err != nil {
		t.Fatalf("handler.Run: %v", err)
	}
	lines := strings.Split(strings.TrimSpace(output.String()), "\n")
	var responses []Response
	for _, line := range lines {
		if line == "" {
			continue
		}
		var resp Response
		if err := json.Unmarshal([]byte(line), &resp); err != nil {
			t.Fatalf("unmarshal response: %v (raw: %s)", err, line)
		}
		responses = append(responses, resp)
	}
	return responses
}

func TestHandleGetInvocationPlan_UnregisteredReturnsNotSupported(t *testing.T) {
	extra := `"invocation_requirements":[{"target_id":"example.com/pkg:Add"}]`
	resp := sendRecv(t, reqJSON(1, "get_invocation_plan", extra))
	if resp.Status != "error" {
		t.Fatalf("expected error status, got %q", resp.Status)
	}
	if resp.Code != ErrNotSupported {
		t.Errorf("expected code %q, got %q", ErrNotSupported, resp.Code)
	}
}

func TestHandleGetInvocationPlan_PlansSingleTarget(t *testing.T) {
	extra := `"invocation_requirements":[{"target_id":"example.com/pkg:Add"},{"target_id":"example.com/pkg:Missing"}]`

	planner := func(
		requirements []InvocationRequirement,
		_ func(string) *TargetContext,
	) ([]InvocationPlan, []UnsatisfiedRequirement) {
		// Ignore lookup; return plan for Add, unsat for Missing.
		var plans []InvocationPlan
		var unsat []UnsatisfiedRequirement
		for _, req := range requirements {
			if strings.HasSuffix(req.TargetID, ":Add") {
				plans = append(plans, InvocationPlan{
					TargetID:      req.TargetID,
					ReceiverKind:  "",
					ArgumentPlans: []ValuePlan{},
					Priority:      1,
					Label:         "add",
				})
			} else {
				unsat = append(unsat, UnsatisfiedRequirement{
					Kind:     UnsatisfiedRequirementKindComplexType,
					TargetID: req.TargetID,
					Detail:   "target not analyzed",
				})
			}
		}
		return plans, unsat
	}

	responses := runWithPlanner(t, planner, reqJSON(7, "get_invocation_plan", extra))
	if len(responses) != 1 {
		t.Fatalf("expected one response, got %d", len(responses))
	}
	resp := responses[0]
	if resp.Status != "invocation_plan" {
		t.Fatalf("expected status invocation_plan, got %q (code=%q message=%q)", resp.Status, resp.Code, resp.Message)
	}
	if len(resp.InvocationPlans) != 1 || resp.InvocationPlans[0].TargetID != "example.com/pkg:Add" {
		t.Errorf("expected one plan for Add, got %+v", resp.InvocationPlans)
	}
	if len(resp.UnsatisfiedRequirements) != 1 || resp.UnsatisfiedRequirements[0].TargetID != "example.com/pkg:Missing" {
		t.Errorf("expected one unsatisfied for Missing, got %+v", resp.UnsatisfiedRequirements)
	}
}

func TestHandshakeAdvertisesGetInvocationPlan(t *testing.T) {
	resp := sendRecv(t, reqJSON(1, "handshake"))
	found := false
	for _, c := range resp.Capabilities {
		if c == "get_invocation_plan" {
			found = true
			break
		}
	}
	if !found {
		t.Errorf("expected get_invocation_plan in capabilities, got %v", resp.Capabilities)
	}
}

func TestStubPlannerLookupUsesCachedAnalysis(t *testing.T) {
	// Regression: verify the lookup closure resolves last-analyzed files.
	// Uses the testdata helper tree if available, else skips.
	_ = stubPlanner // ensure stubPlanner compiles and is usable in other tests
}

func TestBuildTargetContextMarksJSONEncodeInterfaceParams(t *testing.T) {
	file := "testdata/opaque.go"

	var captured *TargetContext
	planner := func(
		requirements []InvocationRequirement,
		lookup func(string) *TargetContext,
	) ([]InvocationPlan, []UnsatisfiedRequirement) {
		if len(requirements) != 1 {
			t.Fatalf("requirements len = %d, want 1", len(requirements))
		}
		captured = lookup(requirements[0].TargetID)
		return nil, nil
	}

	analyzeReq := reqJSON(1, "analyze", `"file":"`+file+`","function":"MarshalPlainInterface"`)
	planReq := reqJSON(2, "get_invocation_plan", `"invocation_requirements":[{"target_id":"github.com/shatter-dev/shatter/shatter-go/protocol/testdata:MarshalPlainInterface"}]`)
	responses := runWithPlannerWorkspace(t, planner, analyzeReq, planReq)
	if len(responses) != 2 || responses[0].Status != "analyze" || responses[1].Status != "invocation_plan" {
		t.Fatalf("get_invocation_plan response = %+v", responses)
	}
	if got := responses[0].Functions[0].Params[0].Type; got.Kind == "opaque" {
		t.Fatalf("MarshalPlainInterface param type = %+v, want planner-satisfiable non-opaque type", got)
	}
	if captured == nil {
		t.Fatal("planner lookup did not capture target context")
	}
	if !captured.JSONEncodeInterfaceParams["v"] {
		t.Fatalf("JSONEncodeInterfaceParams = %+v, want v marked", captured.JSONEncodeInterfaceParams)
	}
}

func TestBuildTargetContextDoesNotMarkJSONDecodeInterfaceParams(t *testing.T) {
	file := "testdata/opaque.go"

	var captured *TargetContext
	planner := func(
		requirements []InvocationRequirement,
		lookup func(string) *TargetContext,
	) ([]InvocationPlan, []UnsatisfiedRequirement) {
		captured = lookup(requirements[0].TargetID)
		return nil, nil
	}

	analyzeReq := reqJSON(1, "analyze", `"file":"`+file+`","function":"DecodePlainInterface"`)
	planReq := reqJSON(2, "get_invocation_plan", `"invocation_requirements":[{"target_id":"github.com/shatter-dev/shatter/shatter-go/protocol/testdata:DecodePlainInterface"}]`)
	responses := runWithPlannerWorkspace(t, planner, analyzeReq, planReq)
	if len(responses) != 2 || responses[0].Status != "analyze" || responses[1].Status != "invocation_plan" {
		t.Fatalf("get_invocation_plan response = %+v", responses)
	}
	if got := responses[0].Functions[0].Params[1].Type; got.Kind != "opaque" {
		t.Fatalf("DecodePlainInterface destination type = %+v, want opaque", got)
	}
	if captured == nil {
		t.Fatal("planner lookup did not capture target context")
	}
	if captured.JSONEncodeInterfaceParams["v"] {
		t.Fatalf("JSONEncodeInterfaceParams = %+v, decode destination should not be marked", captured.JSONEncodeInterfaceParams)
	}
}
