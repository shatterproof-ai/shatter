package planner

import (
	"sort"
	"strings"

	"github.com/shatter-dev/shatter/shatter-go/protocol"
)

// Additive scoring weights for ScoreConstructor. See str-hy9b.F3.
const (
	// ConstructorScoreSamePackage rewards candidates defined in the same
	// package as the target method. Same-package constructors are always
	// preferred over nearby-package alternatives because they cannot
	// introduce cross-package import cycles or visibility issues.
	ConstructorScoreSamePackage = 3
	// ConstructorScoreReturnMatches rewards an exact match between the
	// candidate's TargetType and the receiver type the method needs.
	ConstructorScoreReturnMatches = 2
	// ConstructorScoreZeroParam rewards constructors with no parameters;
	// they are trivially satisfiable and introduce no planner load.
	ConstructorScoreZeroParam = 1
	// ConstructorScoreSatisfiableParam is added for each parameter the
	// planner can satisfy from the primitive families or the runtime-value
	// registry. Parameters that are neither primitive nor registered cost
	// nothing (they are not penalised, but they do not contribute).
	ConstructorScoreSatisfiableParam = 1
	// ConstructorScoreIdiomaticPrefix rewards the standard Go constructor
	// prefixes `New` and `Default` (as in `NewService` / `DefaultConfig`).
	// `Must`-prefixed candidates do not receive this bonus.
	ConstructorScoreIdiomaticPrefix = 1
	// ConstructorScoreReturnsError penalises constructors that return a
	// trailing `error` result; a non-nil error at construction time is a
	// first-class failure mode the planner would rather avoid.
	ConstructorScoreReturnsError = -1
	// ConstructorScoreMustPrefix penalises the `Must<Type>` convention,
	// which panics on failure rather than returning an error — unsuitable
	// for exploratory invocation.
	ConstructorScoreMustPrefix = -2
)

// ScoreConstructorOptions bundles the caller-supplied context required to
// score a single constructor candidate.
type ScoreConstructorOptions struct {
	// NeededType is the bare receiver type name the method requires
	// (e.g. `Service` or `bytes.Buffer`). An exact match against
	// ConstructorCandidate.TargetType contributes
	// ConstructorScoreReturnMatches. An empty NeededType disables the
	// match bonus.
	NeededType string
	// SamePackage indicates the candidate is defined in the same package
	// as the target method, contributing ConstructorScoreSamePackage.
	SamePackage bool
}

// ScoredConstructor pairs a candidate with its additive score.
type ScoredConstructor struct {
	Candidate protocol.ConstructorCandidate
	Score     int
}

// ScoreConstructor returns the additive score for c under opts. See the
// ConstructorScore* constants for the weighting rules.
func ScoreConstructor(c protocol.ConstructorCandidate, opts ScoreConstructorOptions) int {
	score := 0
	if opts.SamePackage {
		score += ConstructorScoreSamePackage
	}
	if opts.NeededType != "" && c.TargetType == opts.NeededType {
		score += ConstructorScoreReturnMatches
	}
	if len(c.Parameters) == 0 {
		score += ConstructorScoreZeroParam
	}
	for _, p := range c.Parameters {
		if isParamSatisfiable(p) {
			score += ConstructorScoreSatisfiableParam
		}
	}
	if hasMustPrefix(c.FuncName) {
		score += ConstructorScoreMustPrefix
	} else if hasIdiomaticConstructorPrefix(c.FuncName) {
		score += ConstructorScoreIdiomaticPrefix
	}
	if c.ReturnsError {
		score += ConstructorScoreReturnsError
	}
	return score
}

// RankConstructors returns cands sorted by descending score, with ties
// broken deterministically by ascending FuncName. The input slice is not
// mutated. Each returned ScoredConstructor carries the exact score produced
// by ScoreConstructor under the same opts.
func RankConstructors(cands []protocol.ConstructorCandidate, opts ScoreConstructorOptions) []ScoredConstructor {
	if len(cands) == 0 {
		return nil
	}
	out := make([]ScoredConstructor, len(cands))
	for i, c := range cands {
		out[i] = ScoredConstructor{Candidate: c, Score: ScoreConstructor(c, opts)}
	}
	sort.SliceStable(out, func(i, j int) bool {
		if out[i].Score != out[j].Score {
			return out[i].Score > out[j].Score
		}
		return out[i].Candidate.FuncName < out[j].Candidate.FuncName
	})
	return out
}

// isParamSatisfiable reports whether the parameter planner can produce at
// least one ValuePlan for p — either from a primitive family or from the
// runtime-value registry. Parameters with neither path available are
// treated as unsatisfiable for scoring purposes.
func isParamSatisfiable(p protocol.ParamInfo) bool {
	if _, ok := classifyParamFamily(p); ok {
		return true
	}
	if plans := runtimeValuePlans(0, p, 1); len(plans) > 0 {
		return true
	}
	return false
}

// hasIdiomaticConstructorPrefix reports whether name begins with one of the
// standard Go constructor prefixes — `New` or `Default` — excluding the
// `Must`-prefixed variants penalised separately.
func hasIdiomaticConstructorPrefix(name string) bool {
	return strings.HasPrefix(name, "New") || strings.HasPrefix(name, "Default")
}

// hasMustPrefix reports whether name begins with the `Must` convention
// (e.g. `MustNewService`).
func hasMustPrefix(name string) bool {
	return strings.HasPrefix(name, "Must")
}
