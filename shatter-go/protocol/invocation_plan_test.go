package protocol_test

import (
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
