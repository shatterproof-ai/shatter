package planner_test

import (
	"strings"
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/planner"
	"github.com/shatter-dev/shatter/shatter-go/protocol"
	"pgregory.net/rapid"
)

// interfaceParam returns a ParamInfo shaped like an interface-typed parameter
// (kind=object, TypeName=interface name).
func interfaceParam(name, ifaceName string) protocol.ParamInfo {
	tn := ifaceName
	return protocol.ParamInfo{
		Name:     name,
		Type:     protocol.TypeInfo{Kind: "object"},
		TypeName: &tn,
	}
}

// AC (str-hy9b.F4): given an interface Store with one in-package MemoryStore
// impl (whose constructor is NewMemoryStore), the planner emits a plan using
// NewMemoryStore().
func TestPlanInterfaceImpls_AcceptanceCriterion(t *testing.T) {
	cand := planner.InterfaceImplCandidate{
		TypeName:    "MemoryStore",
		SamePackage: true,
		Constructors: []protocol.ConstructorCandidate{
			{FuncName: "NewMemoryStore", TargetType: "MemoryStore"},
		},
	}
	plans, unsat := planner.PlanInterfaceImpls(
		"target/store.go:Use",
		0,
		interfaceParam("s", "Store"),
		planner.PlanInterfaceImplOptions{
			InterfaceName: "Store",
			Candidates:    []planner.InterfaceImplCandidate{cand},
		},
	)
	if unsat != nil {
		t.Fatalf("unexpected unsatisfied: %+v", unsat)
	}
	if len(plans) != 1 {
		t.Fatalf("len(plans)=%d, want 1", len(plans))
	}
	got := plans[0]
	if got.Kind != protocol.ValuePlanKindRuntimeValue {
		t.Errorf("Kind=%q, want %q", got.Kind, protocol.ValuePlanKindRuntimeValue)
	}
	if got.ParamIndex != 0 || got.ParamName != "s" {
		t.Errorf("ParamIndex=%d ParamName=%q, want 0/\"s\"", got.ParamIndex, got.ParamName)
	}
	if got.TypeHint != "Store" {
		t.Errorf("TypeHint=%q, want Store", got.TypeHint)
	}
	if string(got.Literal) != `"NewMemoryStore()"` {
		t.Errorf("Literal=%s, want \"NewMemoryStore()\"", string(got.Literal))
	}
}

func TestPlanInterfaceImpls_NoCandidates(t *testing.T) {
	plans, unsat := planner.PlanInterfaceImpls(
		"t",
		0,
		interfaceParam("s", "Store"),
		planner.PlanInterfaceImplOptions{InterfaceName: "Store"},
	)
	if plans != nil {
		t.Errorf("plans=%+v, want nil", plans)
	}
	if unsat == nil {
		t.Fatal("unsat=nil, want non-nil")
	}
	if unsat.Kind != protocol.UnsatisfiedRequirementKindNoConstructor {
		t.Errorf("unsat.Kind=%q, want no_constructor", unsat.Kind)
	}
	if !strings.Contains(unsat.Detail, "Store") {
		t.Errorf("Detail=%q must mention interface name", unsat.Detail)
	}
}

func TestPlanInterfaceImpls_CandidateWithoutConstructor(t *testing.T) {
	plans, unsat := planner.PlanInterfaceImpls(
		"t",
		0,
		interfaceParam("s", "Store"),
		planner.PlanInterfaceImplOptions{
			InterfaceName: "Store",
			Candidates: []planner.InterfaceImplCandidate{
				{TypeName: "FileStore", SamePackage: true},
			},
		},
	)
	if plans != nil {
		t.Errorf("plans=%+v, want nil", plans)
	}
	if unsat == nil || unsat.Kind != protocol.UnsatisfiedRequirementKindNoConstructor {
		t.Fatalf("unsat=%+v, want no_constructor", unsat)
	}
}

func TestPlanInterfaceImpls_RanksSamePackageFirst(t *testing.T) {
	external := planner.InterfaceImplCandidate{
		TypeName:    "DefaultStore",
		SamePackage: false,
		Constructors: []protocol.ConstructorCandidate{
			{FuncName: "NewDefaultStore", TargetType: "DefaultStore"},
		},
	}
	local := planner.InterfaceImplCandidate{
		TypeName:    "ZStore",
		SamePackage: true,
		Constructors: []protocol.ConstructorCandidate{
			{FuncName: "NewZStore", TargetType: "ZStore"},
		},
	}
	plans, unsat := planner.PlanInterfaceImpls(
		"t",
		0,
		interfaceParam("s", "Store"),
		planner.PlanInterfaceImplOptions{
			InterfaceName: "Store",
			Candidates:    []planner.InterfaceImplCandidate{external, local},
		},
	)
	if unsat != nil {
		t.Fatalf("unsat=%+v", unsat)
	}
	if len(plans) != 2 {
		t.Fatalf("len(plans)=%d, want 2", len(plans))
	}
	if string(plans[0].Literal) != `"NewZStore()"` {
		t.Errorf("plans[0].Literal=%s, want NewZStore()", plans[0].Literal)
	}
}

// Among same-package candidates, name tokens (Default/Memory/Mock/Stub)
// outweigh alphabetical order.
func TestPlanInterfaceImpls_NameTokensOutweighAlphabetical(t *testing.T) {
	defaultImpl := planner.InterfaceImplCandidate{
		TypeName:    "ZDefault",
		SamePackage: true,
		Constructors: []protocol.ConstructorCandidate{
			{FuncName: "NewZDefault", TargetType: "ZDefault"},
		},
	}
	plain := planner.InterfaceImplCandidate{
		TypeName:    "AStore",
		SamePackage: true,
		Constructors: []protocol.ConstructorCandidate{
			{FuncName: "NewAStore", TargetType: "AStore"},
		},
	}
	ranked := planner.RankInterfaceImpls(
		[]planner.InterfaceImplCandidate{plain, defaultImpl},
	)
	if len(ranked) != 2 {
		t.Fatalf("len(ranked)=%d", len(ranked))
	}
	if ranked[0].Candidate.TypeName != "ZDefault" {
		t.Errorf("top=%q, want ZDefault", ranked[0].Candidate.TypeName)
	}
}

func TestPlanInterfaceImpls_TopThreeCap(t *testing.T) {
	cands := []planner.InterfaceImplCandidate{
		{TypeName: "DefaultStore", SamePackage: true, Constructors: []protocol.ConstructorCandidate{{FuncName: "NewDefaultStore", TargetType: "DefaultStore"}}},
		{TypeName: "MemoryStore", SamePackage: true, Constructors: []protocol.ConstructorCandidate{{FuncName: "NewMemoryStore", TargetType: "MemoryStore"}}},
		{TypeName: "MockStore", SamePackage: true, Constructors: []protocol.ConstructorCandidate{{FuncName: "NewMockStore", TargetType: "MockStore"}}},
		{TypeName: "StubStore", SamePackage: true, Constructors: []protocol.ConstructorCandidate{{FuncName: "NewStubStore", TargetType: "StubStore"}}},
		{TypeName: "ZStore", SamePackage: true, Constructors: []protocol.ConstructorCandidate{{FuncName: "NewZStore", TargetType: "ZStore"}}},
	}
	plans, unsat := planner.PlanInterfaceImpls(
		"t",
		0,
		interfaceParam("s", "Store"),
		planner.PlanInterfaceImplOptions{InterfaceName: "Store", Candidates: cands},
	)
	if unsat != nil {
		t.Fatalf("unsat=%+v", unsat)
	}
	if len(plans) != planner.DefaultMaxInterfaceImpls {
		t.Errorf("len(plans)=%d, want %d", len(plans), planner.DefaultMaxInterfaceImpls)
	}
}

func TestPlanInterfaceImpls_HonorsMaxImpls(t *testing.T) {
	cands := []planner.InterfaceImplCandidate{
		{TypeName: "DefaultStore", SamePackage: true, Constructors: []protocol.ConstructorCandidate{{FuncName: "NewDefaultStore", TargetType: "DefaultStore"}}},
		{TypeName: "MemoryStore", SamePackage: true, Constructors: []protocol.ConstructorCandidate{{FuncName: "NewMemoryStore", TargetType: "MemoryStore"}}},
	}
	plans, _ := planner.PlanInterfaceImpls(
		"t",
		0,
		interfaceParam("s", "Store"),
		planner.PlanInterfaceImplOptions{InterfaceName: "Store", Candidates: cands, MaxImpls: 1},
	)
	if len(plans) != 1 {
		t.Fatalf("len(plans)=%d, want 1", len(plans))
	}
}

// When two candidates tie in score, alphabetical TypeName breaks the tie.
func TestRankInterfaceImpls_AlphabeticalTieBreak(t *testing.T) {
	a := planner.InterfaceImplCandidate{TypeName: "BStore", SamePackage: true,
		Constructors: []protocol.ConstructorCandidate{{FuncName: "NewBStore", TargetType: "BStore"}}}
	b := planner.InterfaceImplCandidate{TypeName: "AStore", SamePackage: true,
		Constructors: []protocol.ConstructorCandidate{{FuncName: "NewAStore", TargetType: "AStore"}}}
	ranked := planner.RankInterfaceImpls([]planner.InterfaceImplCandidate{a, b})
	if ranked[0].Candidate.TypeName != "AStore" {
		t.Errorf("top=%q, want AStore (alphabetical tiebreak)", ranked[0].Candidate.TypeName)
	}
}

func TestScoreInterfaceImpl_SamePackageBonus(t *testing.T) {
	c := planner.InterfaceImplCandidate{TypeName: "X"}
	with := c
	with.SamePackage = true
	if planner.ScoreInterfaceImpl(with)-planner.ScoreInterfaceImpl(c) != planner.InterfaceImplScoreSamePackage {
		t.Errorf("same-package delta wrong")
	}
}

func TestScoreInterfaceImpl_HasConstructorBonus(t *testing.T) {
	bare := planner.InterfaceImplCandidate{TypeName: "X"}
	withCtor := bare
	withCtor.Constructors = []protocol.ConstructorCandidate{{FuncName: "NewX", TargetType: "X"}}
	delta := planner.ScoreInterfaceImpl(withCtor) - planner.ScoreInterfaceImpl(bare)
	if delta != planner.InterfaceImplScoreHasConstructor {
		t.Errorf("has-constructor delta=%d, want %d", delta, planner.InterfaceImplScoreHasConstructor)
	}
}

func TestRankInterfaceImpls_Empty(t *testing.T) {
	if r := planner.RankInterfaceImpls(nil); r != nil {
		t.Errorf("nil input ranked=%+v, want nil", r)
	}
}

// Property: RankInterfaceImpls is monotonic (non-increasing scores) and
// tie-broken alphabetically; output length equals input length.
func TestRankInterfaceImpls_Invariants(t *testing.T) {
	rapid.Check(t, func(rt *rapid.T) {
		n := rapid.IntRange(0, 8).Draw(rt, "n")
		cands := make([]planner.InterfaceImplCandidate, n)
		for i := range n {
			tokens := []string{"Default", "Memory", "Mock", "Stub", "Plain"}
			tok := rapid.SampledFrom(tokens).Draw(rt, "token")
			cands[i] = planner.InterfaceImplCandidate{
				TypeName:    tok + string(rune('A'+i)),
				SamePackage: rapid.Bool().Draw(rt, "samePkg"),
			}
			if rapid.Bool().Draw(rt, "hasCtor") {
				cands[i].Constructors = []protocol.ConstructorCandidate{
					{FuncName: "New" + cands[i].TypeName, TargetType: cands[i].TypeName},
				}
			}
		}
		ranked := planner.RankInterfaceImpls(cands)
		if len(ranked) != len(cands) {
			t.Fatalf("len(ranked)=%d, want %d", len(ranked), len(cands))
		}
		for i := 1; i < len(ranked); i++ {
			if ranked[i-1].Score < ranked[i].Score {
				t.Fatalf("non-monotonic at %d: %+v", i, ranked)
			}
			if ranked[i-1].Score == ranked[i].Score &&
				ranked[i-1].Candidate.TypeName > ranked[i].Candidate.TypeName {
				t.Fatalf("tie not alphabetical at %d: %+v", i, ranked)
			}
		}
	})
}
