package protocol_test

import (
	"encoding/json"
	"path/filepath"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/planner"
	"github.com/shatter-dev/shatter/shatter-go/protocol"
)

// str-9pkrb: end-to-end within the Go frontend — the analyzer captures a named
// string-alias enum's constant domain (str-pjlc1) and the planner now seeds
// each constant as a string candidate. This ties the real analyzer output for
// the ClassifyColor(c Color) switch fixture to the planner, proving the two
// halves are connected (a unit test on either alone can pass while the pipeline
// stays disconnected — see this project's parallel-path history).
//
// Before this change the planner had no "union" case, so ClassifyColor's Color
// parameter fell to the unsupported path and the RED/GREEN/BLUE switch arms
// were unreachable without a hand-written generator.
func TestNamedStringEnumParam_AnalyzeFeedsPlannerCandidates(t *testing.T) {
	fixture := filepath.Join("testdata", "enum.go")
	results, err := protocol.AnalyzeFile(fixture, "ClassifyColor")
	if err != nil {
		t.Fatalf("AnalyzeFile: %v", err)
	}
	if len(results) == 0 || len(results[0].Params) != 1 {
		t.Fatalf("expected one analyzed param, got %+v", results)
	}
	param := results[0].Params[0]
	if param.Type.Kind != "union" {
		t.Fatalf("analyzer param kind = %q, want union (str-pjlc1 enum domain)", param.Type.Kind)
	}

	plans, u := planner.PlanParam("example.com/pkg:ClassifyColor", 0, param, planner.ParamPlanOptions{})
	if u != nil {
		t.Fatalf("planner returned unsatisfied for a named string enum: %+v", u)
	}

	got := map[string]bool{}
	for _, pl := range plans {
		if pl.Kind == protocol.ValuePlanKindLiteral {
			var s string
			if json.Unmarshal(pl.Literal, &s) == nil {
				got[s] = true
			}
		}
	}
	for _, want := range []string{"RED", "GREEN", "BLUE"} {
		if !got[want] {
			t.Errorf("enum constant %q missing from generated candidates %+v", want, plans)
		}
	}
}
