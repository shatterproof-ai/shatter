// Package planner — F4: same-package interface implementation planning.
//
// When a parameter (or receiver) is typed as an interface, the analyzer scans
// pkg.TypesInfo for concrete types whose method set satisfies the interface
// (see protocol/analyzer.go fileContext.implementors). Those discoveries are
// surfaced to the planner as InterfaceImplCandidate values; this file ranks
// them and emits ValuePlans that call a chosen impl's constructor.
//
// Same-package implementations are preferred. Among siblings with equal
// package-locality, the implementor name is scored against well-known tokens
// (Default / Memory / Mock / Stub) before falling back to alphabetical order;
// the top three candidates are returned as alternatives.
//
// Spec: docs/specs/2026-04-17-go-frontend-redesign-v2.md §F4. Acceptance:
// interface Store with one in-package MemoryStore impl whose constructor is
// NewMemoryStore yields a plan whose argument expression is `NewMemoryStore()`.
package planner

import (
	"encoding/json"
	"sort"
	"strings"

	"github.com/shatter-dev/shatter/shatter-go/protocol"
)

// Additive scoring weights for ScoreInterfaceImpl.
const (
	// InterfaceImplScoreSamePackage rewards implementors defined in the same
	// package as the target consuming the interface. Same-package impls
	// avoid cross-package import wiring in generated wrappers and match the
	// F4 spec's "prefer same-package impls" requirement.
	InterfaceImplScoreSamePackage = 4
	// InterfaceImplScoreNameDefault rewards the conventional `Default*`
	// implementor naming (e.g. DefaultStore).
	InterfaceImplScoreNameDefault = 3
	// InterfaceImplScoreNameMemory rewards the conventional `*Memory*`
	// in-memory implementor naming (e.g. MemoryStore).
	InterfaceImplScoreNameMemory = 3
	// InterfaceImplScoreNameMock rewards the conventional `*Mock*` test
	// implementor naming.
	InterfaceImplScoreNameMock = 2
	// InterfaceImplScoreNameStub rewards the conventional `*Stub*` test
	// implementor naming.
	InterfaceImplScoreNameStub = 2
	// InterfaceImplScoreHasConstructor rewards candidates that carry at
	// least one constructor, since composite-literal / zero-value impl
	// synthesis is reserved for a follow-up story.
	InterfaceImplScoreHasConstructor = 1
)

// preferredImplNameTokens is the ordered substring list used to score
// implementor names. The first match contributes its weight; later matches
// do not stack. Tokens are matched case-insensitively against the impl's
// TypeName.
var preferredImplNameTokens = []struct {
	Token string
	Score int
}{
	{"Default", InterfaceImplScoreNameDefault},
	{"Memory", InterfaceImplScoreNameMemory},
	{"Mock", InterfaceImplScoreNameMock},
	{"Stub", InterfaceImplScoreNameStub},
}

// DefaultMaxInterfaceImpls caps the number of impl alternatives returned by
// PlanInterfaceImpls when PlanInterfaceImplOptions.MaxImpls is zero. Three
// matches the F4 spec's "top three by name match" requirement.
const DefaultMaxInterfaceImpls = 3

// InterfaceImplCandidate names a concrete type whose method set satisfies an
// interface, together with any constructor functions that produce it.
//
// Candidates are produced upstream by the analyzer (which has access to
// pkg.TypesInfo and types.Implements); this planner package does not depend
// on go/types directly.
type InterfaceImplCandidate struct {
	// TypeName is the bare name of the concrete implementor (e.g.
	// "MemoryStore").
	TypeName string
	// SamePackage is true when the implementor is defined in the same
	// package as the target consuming the interface.
	SamePackage bool
	// Constructors lists the known constructor functions that return
	// TypeName, in analyzer order. The interface-impl planner ranks
	// constructors via the F3 ScoreConstructor weighting and uses the
	// top one when emitting a ValuePlan.
	Constructors []protocol.ConstructorCandidate
}

// ScoredInterfaceImpl pairs a candidate with the additive score produced by
// ScoreInterfaceImpl.
type ScoredInterfaceImpl struct {
	Candidate InterfaceImplCandidate
	Score     int
}

// ScoreInterfaceImpl returns the additive preference score for c. Factors:
//   - Same-package preference (InterfaceImplScoreSamePackage)
//   - Name-token preference (Default / Memory / Mock / Stub; first match wins)
//   - Has-constructor bonus (InterfaceImplScoreHasConstructor)
//
// The function is deterministic and depends only on c.
func ScoreInterfaceImpl(c InterfaceImplCandidate) int {
	score := 0
	if c.SamePackage {
		score += InterfaceImplScoreSamePackage
	}
	score += nameTokenScore(c.TypeName)
	if len(c.Constructors) > 0 {
		score += InterfaceImplScoreHasConstructor
	}
	return score
}

func nameTokenScore(name string) int {
	lower := strings.ToLower(name)
	for _, t := range preferredImplNameTokens {
		if strings.Contains(lower, strings.ToLower(t.Token)) {
			return t.Score
		}
	}
	return 0
}

// RankInterfaceImpls returns cands sorted by descending score, with ties
// broken deterministically by ascending TypeName. The input slice is not
// mutated. Each returned ScoredInterfaceImpl carries the exact score
// produced by ScoreInterfaceImpl.
func RankInterfaceImpls(cands []InterfaceImplCandidate) []ScoredInterfaceImpl {
	if len(cands) == 0 {
		return nil
	}
	out := make([]ScoredInterfaceImpl, len(cands))
	for i, c := range cands {
		out[i] = ScoredInterfaceImpl{Candidate: c, Score: ScoreInterfaceImpl(c)}
	}
	sort.SliceStable(out, func(i, j int) bool {
		if out[i].Score != out[j].Score {
			return out[i].Score > out[j].Score
		}
		return out[i].Candidate.TypeName < out[j].Candidate.TypeName
	})
	return out
}

// PlanInterfaceImplOptions bundles caller inputs for interface-impl planning.
type PlanInterfaceImplOptions struct {
	// InterfaceName is the interface type the parameter is typed as
	// (e.g. "Store"). Used as the ValuePlan TypeHint and as the prefix for
	// UnsatisfiedRequirement.Detail. May be empty, in which case the
	// chosen implementor's TypeName is used as the TypeHint.
	InterfaceName string
	// Candidates is the discovered set of concrete types whose method set
	// satisfies the interface, with their known constructors.
	Candidates []InterfaceImplCandidate
	// MaxImpls caps the number of returned ValuePlans. Zero means
	// DefaultMaxInterfaceImpls (the F4 "top three" budget).
	MaxImpls int
}

// PlanInterfaceImpls returns prioritised ValuePlans for an interface-typed
// parameter, one ValuePlan per selected implementor (top-N by
// RankInterfaceImpls, capped at opts.MaxImpls). Each ValuePlan encodes a
// constructor call expression (e.g. "NewMemoryStore()") via
// ValuePlanKindRuntimeValue, mirroring runtimeValuePlans so downstream code
// generators paste the expression verbatim at the argument position.
//
// Returns (nil, UnsatisfiedRequirementKindNoConstructor) when there are no
// candidates or none carry a constructor. Composite-literal and useful-
// zero-value synthesis for impls without a constructor is deferred to a
// follow-up story.
func PlanInterfaceImpls(targetID string, paramIndex int, p protocol.ParamInfo, opts PlanInterfaceImplOptions) ([]protocol.ValuePlan, *protocol.UnsatisfiedRequirement) {
	if len(opts.Candidates) == 0 {
		return nil, &protocol.UnsatisfiedRequirement{
			Kind:     protocol.UnsatisfiedRequirementKindNoConstructor,
			TargetID: targetID,
			Detail:   interfaceImplDetail(opts.InterfaceName, "no implementor discovered"),
		}
	}
	maxImpls := opts.MaxImpls
	if maxImpls <= 0 {
		maxImpls = DefaultMaxInterfaceImpls
	}

	ranked := RankInterfaceImpls(opts.Candidates)
	plans := make([]protocol.ValuePlan, 0, maxImpls)
	for _, scored := range ranked {
		if len(plans) >= maxImpls {
			break
		}
		ctor, ok := pickConstructor(scored.Candidate)
		if !ok {
			continue
		}
		expr := ctor.FuncName + "()"
		literal, err := json.Marshal(expr)
		if err != nil {
			// json.Marshal on a valid UTF-8 string cannot fail; defensive.
			continue
		}
		typeHint := opts.InterfaceName
		if typeHint == "" {
			typeHint = scored.Candidate.TypeName
		}
		plans = append(plans, protocol.ValuePlan{
			ParamIndex: paramIndex,
			ParamName:  p.Name,
			Kind:       protocol.ValuePlanKindRuntimeValue,
			Literal:    literal,
			TypeHint:   typeHint,
		})
	}
	if len(plans) == 0 {
		return nil, &protocol.UnsatisfiedRequirement{
			Kind:     protocol.UnsatisfiedRequirementKindNoConstructor,
			TargetID: targetID,
			Detail:   interfaceImplDetail(opts.InterfaceName, "no constructor discovered for any implementor"),
		}
	}
	return plans, nil
}

// pickConstructor returns the highest-ranked constructor for the candidate,
// using the F3 ScoreConstructor weighting with NeededType=c.TypeName and
// SamePackage=c.SamePackage. Returns ok=false when c has no constructors.
func pickConstructor(c InterfaceImplCandidate) (protocol.ConstructorCandidate, bool) {
	if len(c.Constructors) == 0 {
		return protocol.ConstructorCandidate{}, false
	}
	scoreOpts := ScoreConstructorOptions{
		NeededType:  c.TypeName,
		SamePackage: c.SamePackage,
	}
	ranked := RankConstructors(c.Constructors, scoreOpts)
	if len(ranked) == 0 {
		return protocol.ConstructorCandidate{}, false
	}
	return ranked[0].Candidate, true
}

func interfaceImplDetail(interfaceName, reason string) string {
	if interfaceName == "" {
		return reason
	}
	return "interface " + interfaceName + ": " + reason
}
