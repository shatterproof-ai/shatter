package protocol_test

import (
	"bytes"
	"encoding/json"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/protocol"
)

// ---- ValueRequirement round-trip ----

func TestValueRequirementRoundTrip(t *testing.T) {
	orig := protocol.ValueRequirement{
		ParamIndex: 1,
		ParamName:  "n",
		TypeName:   "int",
		Kind:       protocol.ValueRequirementKindNonZero,
	}
	data, err := json.Marshal(orig)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	var got protocol.ValueRequirement
	if err := json.Unmarshal(data, &got); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if got.ParamIndex != orig.ParamIndex || got.ParamName != orig.ParamName ||
		got.TypeName != orig.TypeName || got.Kind != orig.Kind {
		t.Errorf("roundtrip mismatch: got %+v, want %+v", got, orig)
	}
}

func TestValueRequirementSpecificLiteral(t *testing.T) {
	lit, _ := json.Marshal(42)
	orig := protocol.ValueRequirement{
		ParamIndex: 0,
		ParamName:  "x",
		TypeName:   "int",
		Kind:       protocol.ValueRequirementKindSpecific,
		Literal:    lit,
	}
	data, _ := json.Marshal(orig)
	var got protocol.ValueRequirement
	if err := json.Unmarshal(data, &got); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if string(got.Literal) != "42" {
		t.Errorf("Literal = %s, want 42", got.Literal)
	}
}

// ---- RuntimeRequirement round-trip ----

func TestRuntimeRequirementRoundTrip(t *testing.T) {
	orig := protocol.RuntimeRequirement{
		Kind:     protocol.RuntimeRequirementKindReceiverConstruction,
		TypeName: "Counter",
		Detail:   "pointer receiver requires construction",
	}
	data, _ := json.Marshal(orig)
	var got protocol.RuntimeRequirement
	if err := json.Unmarshal(data, &got); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if got != orig {
		t.Errorf("roundtrip mismatch: got %+v, want %+v", got, orig)
	}
}

// ---- InvocationRequirement round-trip ----

func TestInvocationRequirementRoundTrip(t *testing.T) {
	orig := protocol.InvocationRequirement{
		TargetID: "example.com/pkg:(*Counter).Inc",
		ValueRequirements: []protocol.ValueRequirement{
			{ParamIndex: 0, ParamName: "delta", TypeName: "int", Kind: protocol.ValueRequirementKindAny},
		},
		RuntimeRequirements: []protocol.RuntimeRequirement{
			{Kind: protocol.RuntimeRequirementKindReceiverConstruction, TypeName: "Counter"},
		},
	}
	data, _ := json.Marshal(orig)
	var got protocol.InvocationRequirement
	if err := json.Unmarshal(data, &got); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if got.TargetID != orig.TargetID {
		t.Errorf("TargetID = %q, want %q", got.TargetID, orig.TargetID)
	}
	if len(got.ValueRequirements) != 1 {
		t.Errorf("ValueRequirements len = %d, want 1", len(got.ValueRequirements))
	}
	if len(got.RuntimeRequirements) != 1 {
		t.Errorf("RuntimeRequirements len = %d, want 1", len(got.RuntimeRequirements))
	}
}

func TestInvocationRequirementEmptyRequirements(t *testing.T) {
	orig := protocol.InvocationRequirement{
		TargetID:          "example.com/pkg:Add",
		ValueRequirements: []protocol.ValueRequirement{},
	}
	data, _ := json.Marshal(orig)
	var got protocol.InvocationRequirement
	if err := json.Unmarshal(data, &got); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if got.TargetID != orig.TargetID {
		t.Errorf("TargetID mismatch")
	}
}

// ---- ValuePlan round-trip ----

func TestValuePlanLiteralRoundTrip(t *testing.T) {
	lit, _ := json.Marshal("hello")
	orig := protocol.ValuePlan{
		ParamIndex: 0,
		ParamName:  "s",
		Kind:       protocol.ValuePlanKindLiteral,
		Literal:    lit,
		TypeHint:   "string",
	}
	data, _ := json.Marshal(orig)
	var got protocol.ValuePlan
	if err := json.Unmarshal(data, &got); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if got.Kind != orig.Kind || string(got.Literal) != string(orig.Literal) {
		t.Errorf("roundtrip mismatch: got %+v, want %+v", got, orig)
	}
}

func TestValuePlanZeroRoundTrip(t *testing.T) {
	orig := protocol.ValuePlan{
		ParamIndex: 1,
		Kind:       protocol.ValuePlanKindZero,
		TypeHint:   "int",
	}
	data, _ := json.Marshal(orig)
	var got protocol.ValuePlan
	if err := json.Unmarshal(data, &got); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if got.Kind != protocol.ValuePlanKindZero {
		t.Errorf("Kind = %q, want %q", got.Kind, protocol.ValuePlanKindZero)
	}
}

// ---- InvocationPlan round-trip ----

func TestInvocationPlanRoundTrip(t *testing.T) {
	lit, _ := json.Marshal(0)
	orig := protocol.InvocationPlan{
		TargetID:     "example.com/pkg:Add",
		ReceiverKind: "",
		ArgumentPlans: []protocol.ValuePlan{
			{ParamIndex: 0, ParamName: "a", Kind: protocol.ValuePlanKindLiteral, Literal: lit, TypeHint: "int"},
			{ParamIndex: 1, ParamName: "b", Kind: protocol.ValuePlanKindZero, TypeHint: "int"},
		},
		Priority: 1,
		Label:    "zero_args",
	}
	data, _ := json.Marshal(orig)
	var got protocol.InvocationPlan
	if err := json.Unmarshal(data, &got); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if got.TargetID != orig.TargetID {
		t.Errorf("TargetID = %q, want %q", got.TargetID, orig.TargetID)
	}
	if len(got.ArgumentPlans) != 2 {
		t.Errorf("ArgumentPlans len = %d, want 2", len(got.ArgumentPlans))
	}
	if got.Priority != orig.Priority {
		t.Errorf("Priority = %d, want %d", got.Priority, orig.Priority)
	}
	if got.Label != orig.Label {
		t.Errorf("Label = %q, want %q", got.Label, orig.Label)
	}
}

func TestInvocationPlanMethodReceiver(t *testing.T) {
	orig := protocol.InvocationPlan{
		TargetID:      "example.com/pkg:(*Counter).Inc",
		ReceiverKind:  "constructor:NewCounter",
		ArgumentPlans: []protocol.ValuePlan{},
		Priority:      0,
	}
	data, _ := json.Marshal(orig)
	var got protocol.InvocationPlan
	if err := json.Unmarshal(data, &got); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if got.ReceiverKind != "constructor:NewCounter" {
		t.Errorf("ReceiverKind = %q, want %q", got.ReceiverKind, "constructor:NewCounter")
	}
}

// ---- ReceiverFieldPlan round-trip (str-mhinv.1) ----

func TestInvocationPlanReceiverFieldPlansRoundTrip(t *testing.T) {
	backendLit, _ := json.Marshal("newFakeSearchBackend()")
	maxLit, _ := json.Marshal(10)
	orig := protocol.InvocationPlan{
		TargetID:      "example.com/pkg:(*queryResolver).Search",
		ReceiverKind:  "constructor:NewQueryResolver",
		ArgumentPlans: []protocol.ValuePlan{},
		ReceiverFieldPlans: []protocol.ReceiverFieldPlan{
			{
				Path:     []string{"Resolver", "SearchBackend"},
				Kind:     protocol.ValuePlanKindRuntimeValue,
				Literal:  backendLit,
				TypeHint: "*SearchBackend",
			},
			{
				Path:     []string{"MaxResults"},
				Kind:     protocol.ValuePlanKindLiteral,
				Literal:  maxLit,
				TypeHint: "int",
			},
		},
		Priority: 0,
		Label:    "resolver_with_backend",
	}
	data, err := json.Marshal(orig)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	if !bytes.Contains(data, []byte(`"receiver_field_plans"`)) {
		t.Fatalf("expected receiver_field_plans in JSON, got: %s", data)
	}
	if !bytes.Contains(data, []byte(`"path":["Resolver","SearchBackend"]`)) {
		t.Fatalf("expected typed path array in JSON, got: %s", data)
	}
	var got protocol.InvocationPlan
	if err := json.Unmarshal(data, &got); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if len(got.ReceiverFieldPlans) != 2 {
		t.Fatalf("ReceiverFieldPlans len = %d, want 2", len(got.ReceiverFieldPlans))
	}
	first := got.ReceiverFieldPlans[0]
	if len(first.Path) != 2 || first.Path[0] != "Resolver" || first.Path[1] != "SearchBackend" {
		t.Errorf("Path = %v, want [Resolver SearchBackend]", first.Path)
	}
	if first.Kind != protocol.ValuePlanKindRuntimeValue {
		t.Errorf("Kind = %q, want %q", first.Kind, protocol.ValuePlanKindRuntimeValue)
	}
	if string(first.Literal) != string(backendLit) {
		t.Errorf("Literal = %s, want %s", first.Literal, backendLit)
	}
	if first.TypeHint != "*SearchBackend" {
		t.Errorf("TypeHint = %q, want %q", first.TypeHint, "*SearchBackend")
	}
}

// TestInvocationPlanWithoutReceiverFieldPlansBackwardCompatible verifies that a
// plan with no receiver field plans omits the key entirely and that legacy JSON
// lacking the key decodes to a nil/empty slice — preserving the existing wire
// shape.
func TestInvocationPlanWithoutReceiverFieldPlansBackwardCompatible(t *testing.T) {
	orig := protocol.InvocationPlan{
		TargetID:      "example.com/pkg:Add",
		ReceiverKind:  "",
		ArgumentPlans: []protocol.ValuePlan{{ParamIndex: 0, ParamName: "a", Kind: protocol.ValuePlanKindZero, TypeHint: "int"}},
		Priority:      1,
		Label:         "zero_args",
	}
	data, err := json.Marshal(orig)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}
	if bytes.Contains(data, []byte("receiver_field_plans")) {
		t.Errorf("empty ReceiverFieldPlans should be omitted from JSON, got: %s", data)
	}

	legacy := `{"target_id":"example.com/pkg:Add","receiver_kind":"","argument_plans":[{"param_index":0,"param_name":"a","kind":"zero","type_hint":"int"}],"priority":1,"label":"zero_args"}`
	var got protocol.InvocationPlan
	if err := json.Unmarshal([]byte(legacy), &got); err != nil {
		t.Fatalf("unmarshal legacy: %v", err)
	}
	if len(got.ReceiverFieldPlans) != 0 {
		t.Errorf("ReceiverFieldPlans len = %d, want 0", len(got.ReceiverFieldPlans))
	}
}

// ---- UnsatisfiedRequirement round-trip ----

func TestUnsatisfiedRequirementRoundTrip(t *testing.T) {
	cases := []protocol.UnsatisfiedRequirementKind{
		protocol.UnsatisfiedRequirementKindNoConstructor,
		protocol.UnsatisfiedRequirementKindInterfaceReceiver,
		protocol.UnsatisfiedRequirementKindGenericUnconstrained,
		protocol.UnsatisfiedRequirementKindCGODependency,
		protocol.UnsatisfiedRequirementKindComplexType,
	}
	for _, kind := range cases {
		orig := protocol.UnsatisfiedRequirement{
			Kind:     kind,
			TargetID: "example.com/pkg:Fn",
			Detail:   "some detail",
		}
		data, _ := json.Marshal(orig)
		var got protocol.UnsatisfiedRequirement
		if err := json.Unmarshal(data, &got); err != nil {
			t.Fatalf("kind %q unmarshal: %v", kind, err)
		}
		if got != orig {
			t.Errorf("kind %q roundtrip mismatch: got %+v, want %+v", kind, got, orig)
		}
	}
}

// ---- Constant spelling tests (ensure JSON tag values are stable) ----

func TestValueRequirementKindJSON(t *testing.T) {
	cases := map[protocol.ValueRequirementKind]string{
		protocol.ValueRequirementKindAny:      `"any"`,
		protocol.ValueRequirementKindNonZero:  `"non_zero"`,
		protocol.ValueRequirementKindPositive: `"positive"`,
		protocol.ValueRequirementKindSpecific: `"specific"`,
	}
	for kind, want := range cases {
		data, _ := json.Marshal(kind)
		if string(data) != want {
			t.Errorf("ValueRequirementKind %v JSON = %s, want %s", kind, data, want)
		}
	}
}

func TestValuePlanKindJSON(t *testing.T) {
	cases := map[protocol.ValuePlanKind]string{
		protocol.ValuePlanKindLiteral:      `"literal"`,
		protocol.ValuePlanKindZero:         `"zero"`,
		protocol.ValuePlanKindRandom:       `"random"`,
		protocol.ValuePlanKindSymbolic:     `"symbolic"`,
		protocol.ValuePlanKindRuntimeValue: `"runtime_value"`,
	}
	for kind, want := range cases {
		data, err := json.Marshal(kind)
		if err != nil {
			t.Fatalf("ValuePlanKind %v marshal error: %v", kind, err)
		}
		if string(data) != want {
			t.Errorf("ValuePlanKind %v JSON = %s, want %s", kind, data, want)
		}
		// Round-trip: decoded form must equal the original kind.
		var got protocol.ValuePlanKind
		if err := json.Unmarshal(data, &got); err != nil {
			t.Fatalf("ValuePlanKind %v unmarshal error: %v", kind, err)
		}
		if got != kind {
			t.Errorf("ValuePlanKind round-trip = %v, want %v", got, kind)
		}
	}
}

func TestUnsatisfiedRequirementKindJSON(t *testing.T) {
	cases := map[protocol.UnsatisfiedRequirementKind]string{
		protocol.UnsatisfiedRequirementKindNoConstructor:      `"no_constructor"`,
		protocol.UnsatisfiedRequirementKindInterfaceReceiver:   `"interface_receiver"`,
		protocol.UnsatisfiedRequirementKindGenericUnconstrained: `"generic_unconstrained"`,
		protocol.UnsatisfiedRequirementKindCGODependency:       `"cgo_dependency"`,
		protocol.UnsatisfiedRequirementKindComplexType:         `"complex_type"`,
	}
	for kind, want := range cases {
		data, _ := json.Marshal(kind)
		if string(data) != want {
			t.Errorf("UnsatisfiedRequirementKind %v JSON = %s, want %s", kind, data, want)
		}
	}
}
