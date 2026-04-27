package planner_test

import (
	"testing"

	"github.com/shatter-dev/shatter/shatter-go/planner"
	"github.com/shatter-dev/shatter/shatter-go/protocol"
	"pgregory.net/rapid"
)

func contextParam(name string) protocol.ParamInfo {
	tn := "context.Context"
	return protocol.ParamInfo{Name: name, Type: protocol.TypeInfo{Kind: "object"}, TypeName: &tn}
}

// AC: Two constructors of *Service — one with satisfiable parameters scores
// strictly higher than one whose parameters the planner cannot satisfy.
func TestScoreConstructor_AcceptanceCriterion(t *testing.T) {
	opts := planner.ScoreConstructorOptions{NeededType: "Service", SamePackage: true}

	satisfiable := protocol.ConstructorCandidate{
		FuncName:   "NewService",
		TargetType: "Service",
		Parameters: []protocol.ParamInfo{strParam("name"), intParam("port")},
	}
	unsatisfiable := protocol.ConstructorCandidate{
		FuncName:   "NewServiceFromConfig",
		TargetType: "Service",
		Parameters: []protocol.ParamInfo{opaqueParam("cfg", "*Config")},
	}

	satScore := planner.ScoreConstructor(satisfiable, opts)
	unsatScore := planner.ScoreConstructor(unsatisfiable, opts)
	if satScore <= unsatScore {
		t.Fatalf("satisfiable score (%d) must exceed unsatisfiable score (%d)", satScore, unsatScore)
	}

	ranked := planner.RankConstructors([]protocol.ConstructorCandidate{unsatisfiable, satisfiable}, opts)
	if len(ranked) != 2 {
		t.Fatalf("ranked len = %d, want 2", len(ranked))
	}
	if ranked[0].Candidate.FuncName != "NewService" {
		t.Errorf("top candidate = %q, want NewService", ranked[0].Candidate.FuncName)
	}
}

func TestScoreConstructor_SamePackageBonus(t *testing.T) {
	c := protocol.ConstructorCandidate{FuncName: "Make", TargetType: "X"}
	with := planner.ScoreConstructor(c, planner.ScoreConstructorOptions{SamePackage: true})
	without := planner.ScoreConstructor(c, planner.ScoreConstructorOptions{SamePackage: false})
	if with-without != planner.ConstructorScoreSamePackage {
		t.Errorf("same-package delta = %d, want %d", with-without, planner.ConstructorScoreSamePackage)
	}
}

func TestScoreConstructor_ReturnMatchesNeededType(t *testing.T) {
	c := protocol.ConstructorCandidate{FuncName: "Make", TargetType: "Service"}
	match := planner.ScoreConstructor(c, planner.ScoreConstructorOptions{NeededType: "Service"})
	miss := planner.ScoreConstructor(c, planner.ScoreConstructorOptions{NeededType: "Other"})
	empty := planner.ScoreConstructor(c, planner.ScoreConstructorOptions{})
	if match-miss != planner.ConstructorScoreReturnMatches {
		t.Errorf("match delta = %d, want %d", match-miss, planner.ConstructorScoreReturnMatches)
	}
	if empty != miss {
		t.Errorf("empty NeededType score (%d) should equal mismatch score (%d)", empty, miss)
	}
}

func TestScoreConstructor_ZeroParamBonus(t *testing.T) {
	noArgs := protocol.ConstructorCandidate{FuncName: "Make", TargetType: "X"}
	oneArg := protocol.ConstructorCandidate{
		FuncName:   "Make",
		TargetType: "X",
		Parameters: []protocol.ParamInfo{strParam("name")},
	}
	// oneArg gains +1 per satisfiable param and loses +1 for zero-param,
	// so the net score equals noArgs's zero-param bonus.
	if planner.ScoreConstructor(noArgs, planner.ScoreConstructorOptions{}) != planner.ConstructorScoreZeroParam {
		t.Errorf("zero-arg ctor should score ConstructorScoreZeroParam")
	}
	if planner.ScoreConstructor(oneArg, planner.ScoreConstructorOptions{}) != planner.ConstructorScoreSatisfiableParam {
		t.Errorf("one-satisfiable-arg ctor should score ConstructorScoreSatisfiableParam")
	}
}

func TestScoreConstructor_SatisfiableParamBonus(t *testing.T) {
	base := protocol.ConstructorCandidate{FuncName: "Make", TargetType: "X"}
	twoPrim := base
	twoPrim.Parameters = []protocol.ParamInfo{strParam("a"), intParam("b")}
	oneRegistry := base
	oneRegistry.Parameters = []protocol.ParamInfo{contextParam("ctx")}
	unsat := base
	unsat.Parameters = []protocol.ParamInfo{opaqueParam("cfg", "*Config")}

	if got := planner.ScoreConstructor(twoPrim, planner.ScoreConstructorOptions{}); got != 2*planner.ConstructorScoreSatisfiableParam {
		t.Errorf("two primitive params score = %d, want %d", got, 2*planner.ConstructorScoreSatisfiableParam)
	}
	if got := planner.ScoreConstructor(oneRegistry, planner.ScoreConstructorOptions{}); got != planner.ConstructorScoreSatisfiableParam {
		t.Errorf("registry param score = %d, want %d", got, planner.ConstructorScoreSatisfiableParam)
	}
	if got := planner.ScoreConstructor(unsat, planner.ScoreConstructorOptions{}); got != 0 {
		t.Errorf("unsatisfiable param score = %d, want 0", got)
	}
}

func TestScoreConstructor_IdiomaticPrefixBonus(t *testing.T) {
	// All candidates are zero-arg, so each score includes
	// ConstructorScoreZeroParam plus the idiomatic-prefix bonus when it
	// applies.
	base := planner.ConstructorScoreZeroParam
	cases := []struct {
		name string
		want int
	}{
		{"NewService", base + planner.ConstructorScoreIdiomaticPrefix},
		{"New", base + planner.ConstructorScoreIdiomaticPrefix},
		{"DefaultConfig", base + planner.ConstructorScoreIdiomaticPrefix},
		{"Make", base},
		{"MakeService", base},
	}
	for _, tc := range cases {
		c := protocol.ConstructorCandidate{FuncName: tc.name, TargetType: "X"}
		got := planner.ScoreConstructor(c, planner.ScoreConstructorOptions{})
		if got != tc.want {
			t.Errorf("%s score = %d, want %d", tc.name, got, tc.want)
		}
	}
}

func TestScoreConstructor_ReturnsErrorPenalty(t *testing.T) {
	clean := protocol.ConstructorCandidate{FuncName: "Make", TargetType: "X"}
	erroring := clean
	erroring.ReturnsError = true
	delta := planner.ScoreConstructor(erroring, planner.ScoreConstructorOptions{}) -
		planner.ScoreConstructor(clean, planner.ScoreConstructorOptions{})
	if delta != planner.ConstructorScoreReturnsError {
		t.Errorf("returns-error delta = %d, want %d", delta, planner.ConstructorScoreReturnsError)
	}
}

func TestScoreConstructor_MustPrefixPenalty(t *testing.T) {
	must := protocol.ConstructorCandidate{FuncName: "MustNewService", TargetType: "Service"}
	new := protocol.ConstructorCandidate{FuncName: "NewService", TargetType: "Service"}
	mustScore := planner.ScoreConstructor(must, planner.ScoreConstructorOptions{})
	newScore := planner.ScoreConstructor(new, planner.ScoreConstructorOptions{})
	// MustNewService: +0 zero-param? No parameters -> +1 zero-param, -2 Must = -1
	// NewService: +1 zero-param, +1 idiomatic = +2
	// Delta: new - must = 3 = (ConstructorScoreIdiomaticPrefix - ConstructorScoreMustPrefix)
	wantDelta := planner.ConstructorScoreIdiomaticPrefix - planner.ConstructorScoreMustPrefix
	if newScore-mustScore != wantDelta {
		t.Errorf("New vs Must delta = %d, want %d", newScore-mustScore, wantDelta)
	}
}

func TestScoreConstructor_MustAndIdiomaticAreMutuallyExclusive(t *testing.T) {
	// MustNew starts with both "Must" and a "New"-like substring; only the
	// Must penalty should apply (no idiomatic bonus).
	c := protocol.ConstructorCandidate{FuncName: "MustNewX", TargetType: "X"}
	got := planner.ScoreConstructor(c, planner.ScoreConstructorOptions{})
	want := planner.ConstructorScoreZeroParam + planner.ConstructorScoreMustPrefix
	if got != want {
		t.Errorf("MustNewX score = %d, want %d", got, want)
	}
}

func TestRankConstructors_SortsDescendingByScore(t *testing.T) {
	opts := planner.ScoreConstructorOptions{NeededType: "Service", SamePackage: true}
	low := protocol.ConstructorCandidate{
		FuncName:     "MustNewService",
		TargetType:   "Service",
		ReturnsError: true,
	}
	mid := protocol.ConstructorCandidate{
		FuncName:   "NewServiceFromCfg",
		TargetType: "Service",
		Parameters: []protocol.ParamInfo{opaqueParam("cfg", "*Config")},
	}
	high := protocol.ConstructorCandidate{
		FuncName:   "NewService",
		TargetType: "Service",
		Parameters: []protocol.ParamInfo{strParam("name")},
	}

	ranked := planner.RankConstructors([]protocol.ConstructorCandidate{low, mid, high}, opts)
	if len(ranked) != 3 {
		t.Fatalf("len = %d, want 3", len(ranked))
	}
	for i := 1; i < len(ranked); i++ {
		if ranked[i-1].Score < ranked[i].Score {
			t.Fatalf("ranked not sorted: %+v", ranked)
		}
	}
	if ranked[0].Candidate.FuncName != "NewService" {
		t.Errorf("top = %q, want NewService", ranked[0].Candidate.FuncName)
	}
	if ranked[2].Candidate.FuncName != "MustNewService" {
		t.Errorf("bottom = %q, want MustNewService", ranked[2].Candidate.FuncName)
	}
}

func TestRankConstructors_DeterministicTieBreak(t *testing.T) {
	opts := planner.ScoreConstructorOptions{NeededType: "X", SamePackage: true}
	a := protocol.ConstructorCandidate{FuncName: "NewA", TargetType: "X"}
	b := protocol.ConstructorCandidate{FuncName: "NewB", TargetType: "X"}
	c := protocol.ConstructorCandidate{FuncName: "NewC", TargetType: "X"}

	ranked := planner.RankConstructors([]protocol.ConstructorCandidate{c, a, b}, opts)
	got := []string{ranked[0].Candidate.FuncName, ranked[1].Candidate.FuncName, ranked[2].Candidate.FuncName}
	want := []string{"NewA", "NewB", "NewC"}
	for i := range got {
		if got[i] != want[i] {
			t.Errorf("rank[%d] = %q, want %q", i, got[i], want[i])
		}
	}
}

func TestRankConstructors_EmptyInput(t *testing.T) {
	if ranked := planner.RankConstructors(nil, planner.ScoreConstructorOptions{}); ranked != nil {
		t.Errorf("nil input ranked = %+v, want nil", ranked)
	}
	if ranked := planner.RankConstructors([]protocol.ConstructorCandidate{}, planner.ScoreConstructorOptions{}); ranked != nil {
		t.Errorf("empty input ranked = %+v, want nil", ranked)
	}
}

// Rapid property: RankConstructors is stable (scores are non-increasing) and
// the output length matches the input length.
func TestRankConstructors_Invariants(t *testing.T) {
	rapid.Check(t, func(rt *rapid.T) {
		n := rapid.IntRange(0, 8).Draw(rt, "n")
		cands := make([]protocol.ConstructorCandidate, n)
		for i := range n {
			prefix := rapid.SampledFrom([]string{"New", "Default", "Make", "MustNew"}).Draw(rt, "prefix")
			cands[i] = protocol.ConstructorCandidate{
				FuncName:     prefix + string(rune('A'+i)),
				TargetType:   "X",
				ReturnsError: rapid.Bool().Draw(rt, "err"),
			}
			paramCount := rapid.IntRange(0, 3).Draw(rt, "params")
			for j := 0; j < paramCount; j++ {
				if rapid.Bool().Draw(rt, "satisfiable") {
					cands[i].Parameters = append(cands[i].Parameters, strParam("p"))
				} else {
					cands[i].Parameters = append(cands[i].Parameters, opaqueParam("p", "*Opaque"))
				}
			}
		}
		opts := planner.ScoreConstructorOptions{
			NeededType:  "X",
			SamePackage: rapid.Bool().Draw(rt, "samePkg"),
		}
		ranked := planner.RankConstructors(cands, opts)
		if len(ranked) != len(cands) {
			t.Fatalf("len(ranked)=%d, want %d", len(ranked), len(cands))
		}
		for i := 1; i < len(ranked); i++ {
			if ranked[i-1].Score < ranked[i].Score {
				t.Fatalf("non-monotonic: %+v", ranked)
			}
			if ranked[i-1].Score == ranked[i].Score &&
				ranked[i-1].Candidate.FuncName > ranked[i].Candidate.FuncName {
				t.Fatalf("tie broken non-alphabetically: %+v", ranked)
			}
		}
	})
}
