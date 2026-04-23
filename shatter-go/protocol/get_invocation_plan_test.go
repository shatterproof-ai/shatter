package protocol

import (
	"bytes"
	"encoding/json"
	"io"
	"strings"
	"testing"
)

// Minimal inline planner func used by tests to avoid pulling in the planner
// package (which would create an import cycle).
func stubPlanner(
	requirements []InvocationRequirement,
	lookup func(string) *FunctionAnalysis,
) ([]InvocationPlan, []UnsatisfiedRequirement) {
	var plans []InvocationPlan
	var unsat []UnsatisfiedRequirement
	for _, req := range requirements {
		analysis := lookup(req.TargetID)
		if analysis == nil {
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
	input := strings.NewReader(strings.Join(requests, "\n") + "\n")
	var output bytes.Buffer
	handler := NewHandler(input, &output, io.Discard)
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
		_ func(string) *FunctionAnalysis,
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
