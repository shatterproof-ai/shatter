package planner_test

import (
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/planner"
	"github.com/shatter-dev/shatter/shatter-go/protocol"
)

func TestClassifyFixtureMatrix(t *testing.T) {
	cases := []struct {
		name   string
		target protocol.DiscoveredTarget
		want   planner.TargetClass
	}{
		{
			name:   "direct_function",
			target: makeTarget(protocol.TargetKindFunction),
			want:   planner.DirectFunctionClass{},
		},
		{
			name:   "method",
			target: makeMethodTarget(false),
			want:   planner.MethodClass{},
		},
		{
			name:   "adapter_candidate",
			target: makeTarget(protocol.TargetKindAdapter),
			want:   planner.AdapterCandidateClass{},
		},
		{
			name:   "unsupported/generic_unconstrained",
			target: withTypeParams(makeTarget(protocol.TargetKindFunction)),
			want:   planner.UnsupportedClass{Reason: planner.UnsupportedReasonGenericUnconstrained},
		},
		{
			name:   "unsupported/interface_receiver",
			target: makeMethodTarget(true),
			want:   planner.UnsupportedClass{Reason: planner.UnsupportedReasonInterfaceReceiver},
		},
		{
			name:   "unsupported/cgo_dependency",
			target: withCGoDep(makeTarget(protocol.TargetKindFunction)),
			want:   planner.UnsupportedClass{Reason: planner.UnsupportedReasonCGoDependency},
		},
		{
			name:   "unsupported/test_only_visibility",
			target: withTestFile(makeTarget(protocol.TargetKindFunction)),
			want:   planner.UnsupportedClass{Reason: planner.UnsupportedReasonTestOnlyVisibility},
		},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := planner.Classify(tc.target)
			if got != tc.want {
				t.Errorf("Classify() = %T(%v), want %T(%v)", got, got, tc.want, tc.want)
			}
		})
	}
}

func TestClassifyPriorityOrder(t *testing.T) {
	// generic_unconstrained beats test_only_visibility
	t.Run("generic_unconstrained_beats_test_file", func(t *testing.T) {
		target := withTypeParams(withTestFile(makeTarget(protocol.TargetKindFunction)))
		got := planner.Classify(target)
		want := planner.UnsupportedClass{Reason: planner.UnsupportedReasonGenericUnconstrained}
		if got != want {
			t.Errorf("Classify() = %T(%v), want %T(%v)", got, got, want, want)
		}
	})

	// interface_receiver beats cgo_dependency
	t.Run("interface_receiver_beats_cgo", func(t *testing.T) {
		target := withCGoDep(makeMethodTarget(true))
		got := planner.Classify(target)
		want := planner.UnsupportedClass{Reason: planner.UnsupportedReasonInterfaceReceiver}
		if got != want {
			t.Errorf("Classify() = %T(%v), want %T(%v)", got, got, want, want)
		}
	})
}

func makeTarget(kind protocol.TargetKind) protocol.DiscoveredTarget {
	return protocol.DiscoveredTarget{
		ID:            "example.com/pkg:Foo",
		PackagePath:   "example.com/pkg",
		PackageName:   "pkg",
		FilePath:      "/src/pkg/foo.go",
		SymbolName:    "Foo",
		QualifiedName: "Foo",
		Kind:          kind,
		Parameters:    []protocol.ParamInfo{},
		Results:       []protocol.TypeInfo{},
		Visibility:    "exported",
	}
}

func makeMethodTarget(isInterface bool) protocol.DiscoveredTarget {
	t := makeTarget(protocol.TargetKindMethod)
	t.QualifiedName = "(*Counter).Method"
	t.Receiver = &protocol.ReceiverShape{
		TypeName:    "Counter",
		IsPointer:   true,
		IsInterface: isInterface,
	}
	return t
}

func withTypeParams(t protocol.DiscoveredTarget) protocol.DiscoveredTarget {
	t.HasTypeParams = true
	return t
}

func withCGoDep(t protocol.DiscoveredTarget) protocol.DiscoveredTarget {
	t.HasCGoDep = true
	return t
}

func withTestFile(t protocol.DiscoveredTarget) protocol.DiscoveredTarget {
	t.IsTestFile = true
	return t
}
