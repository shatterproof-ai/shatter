package instrument

import "fmt"

// generateRecorder returns the source code for __shatter_recorder.go
// that provides runtime recording functions for instrumented code.
func generateRecorder(packageName string) string {
	return fmt.Sprintf(`package %s

import (
	"encoding/json"
	"os"
	"sync"
)

type __shatterConditionOutcome struct {
	ConditionIndex int    `+"`"+`json:"condition_index"`+"`"+`
	Value          *bool  `+"`"+`json:"value"`+"`"+`
	Masked         bool   `+"`"+`json:"masked,omitempty"`+"`"+`
	ConstraintJSON string `+"`"+`json:"constraint_json,omitempty"`+"`"+`
}

type __shatterMcdcResult struct {
	decision   bool
	conditions []__shatterConditionOutcome
}

type __shatterBranchDecision struct {
	BranchID       int                         `+"`"+`json:"branch_id"`+"`"+`
	Line           int                         `+"`"+`json:"line"`+"`"+`
	Taken          bool                        `+"`"+`json:"taken"`+"`"+`
	ConstraintJSON string                      `+"`"+`json:"constraint_json,omitempty"`+"`"+`
	Conditions     []__shatterConditionOutcome `+"`"+`json:"conditions,omitempty"`+"`"+`
}

type __shatterScopeEvent struct {
	Kind       string `+"`"+`json:"kind"`+"`"+`
	LoopID     *int   `+"`"+`json:"loop_id,omitempty"`+"`"+`
	CallSiteID *int   `+"`"+`json:"call_site_id,omitempty"`+"`"+`
}

type __shatterTraceEvent struct {
	Type     string                   `+"`"+`json:"type"`+"`"+`
	Decision *__shatterBranchDecision `+"`"+`json:"decision,omitempty"`+"`"+`
	Event    *__shatterScopeEvent     `+"`"+`json:"event,omitempty"`+"`"+`
}

type __shatterResults struct {
	LinesExecuted []int                     `+"`"+`json:"lines_executed"`+"`"+`
	BranchPath    []__shatterBranchDecision `+"`"+`json:"branch_path"`+"`"+`
	ScopeEvents   []__shatterTraceEvent     `+"`"+`json:"scope_events"`+"`"+`
}

var (
	__shatter_mu       sync.Mutex
	__shatter_lines    []int
	__shatter_branches []__shatterBranchDecision
	__shatter_trace    []__shatterTraceEvent
)

func __shatter_record_line(line int) {
	__shatter_mu.Lock()
	__shatter_lines = append(__shatter_lines, line)
	__shatter_mu.Unlock()
}

func __shatter_record_branch(branchID, line int, cond bool, constraintJSON string) bool {
	__shatter_mu.Lock()
	decision := __shatterBranchDecision{
		BranchID:       branchID,
		Line:           line,
		Taken:          cond,
		ConstraintJSON: constraintJSON,
	}
	__shatter_branches = append(__shatter_branches, decision)
	__shatter_trace = append(__shatter_trace, __shatterTraceEvent{
		Type: "branch", Decision: &decision,
	})
	__shatter_mu.Unlock()
	return cond
}

// __shatter_record_branch_mcdc records a compound branch decision with per-condition
// outcomes already computed by __shatter_mcdc_record. Returns the decision value.
func __shatter_record_branch_mcdc(branchID, line int, result __shatterMcdcResult) bool {
	__shatter_mu.Lock()
	decision := __shatterBranchDecision{
		BranchID:   branchID,
		Line:       line,
		Taken:      result.decision,
		Conditions: result.conditions,
	}
	__shatter_branches = append(__shatter_branches, decision)
	__shatter_trace = append(__shatter_trace, __shatterTraceEvent{
		Type: "branch", Decision: &decision,
	})
	__shatter_mu.Unlock()
	return result.decision
}

// __shatter_mcdc_record evaluates condition thunks left-to-right, respecting
// short-circuit semantics, and returns the decision outcome plus per-condition
// ConditionOutcome entries.
//
// operator must be "and" or "or".
// constraints must have the same length as thunks; each entry is the
// pre-serialised JSON for that leaf condition's symConstraint.
// thunks are zero-argument bool-returning functions, one per leaf condition.
// Masked conditions (short-circuited) receive value=nil, masked=true.
func __shatter_mcdc_record(branchID, line int, operator string, constraints []string, thunks ...func() bool) __shatterMcdcResult {
	outcomes := make([]__shatterConditionOutcome, len(thunks))
	decision := false
	stopAfter := -1

	if operator == "and" {
		decision = true
		for i, thunk := range thunks {
			if stopAfter >= 0 {
				// Masked: short-circuit prevents evaluation.
				outcomes[i] = __shatterConditionOutcome{
					ConditionIndex: i,
					Value:          nil,
					Masked:         true,
					ConstraintJSON: safeConstraint(constraints, i),
				}
				continue
			}
			val := thunk()
			outcomes[i] = __shatterConditionOutcome{
				ConditionIndex: i,
				Value:          boolPtr(val),
				ConstraintJSON: safeConstraint(constraints, i),
			}
			if !val {
				decision = false
				stopAfter = i
			}
		}
	} else {
		// "or"
		decision = false
		for i, thunk := range thunks {
			if stopAfter >= 0 {
				outcomes[i] = __shatterConditionOutcome{
					ConditionIndex: i,
					Value:          nil,
					Masked:         true,
					ConstraintJSON: safeConstraint(constraints, i),
				}
				continue
			}
			val := thunk()
			outcomes[i] = __shatterConditionOutcome{
				ConditionIndex: i,
				Value:          boolPtr(val),
				ConstraintJSON: safeConstraint(constraints, i),
			}
			if val {
				decision = true
				stopAfter = i
			}
		}
	}

	return __shatterMcdcResult{decision: decision, conditions: outcomes}
}

func boolPtr(b bool) *bool { return &b }

func safeConstraint(constraints []string, i int) string {
	if i < len(constraints) {
		return constraints[i]
	}
	return ""
}

func __shatter_record_scope(kind string, id int) {
	__shatter_mu.Lock()
	evt := __shatterScopeEvent{Kind: kind}
	if kind == "loop_enter" || kind == "loop_exit" {
		evt.LoopID = &id
	} else {
		evt.CallSiteID = &id
	}
	__shatter_trace = append(__shatter_trace, __shatterTraceEvent{
		Type: "scope", Event: &evt,
	})
	__shatter_mu.Unlock()
}

func __shatter_dump_results(path string) error {
	__shatter_mu.Lock()
	results := __shatterResults{
		LinesExecuted: __shatter_lines,
		BranchPath:    __shatter_branches,
		ScopeEvents:   __shatter_trace,
	}
	if results.LinesExecuted == nil {
		results.LinesExecuted = []int{}
	}
	if results.BranchPath == nil {
		results.BranchPath = []__shatterBranchDecision{}
	}
	if results.ScopeEvents == nil {
		results.ScopeEvents = []__shatterTraceEvent{}
	}
	__shatter_mu.Unlock()
	data, err := json.Marshal(results)
	if err != nil {
		return err
	}
	return os.WriteFile(path, data, 0644)
}

// __shatter_reset clears all recorded data, ready for a new iteration.
func __shatter_reset() {
	__shatter_mu.Lock()
	__shatter_lines = __shatter_lines[:0]
	__shatter_branches = __shatter_branches[:0]
	__shatter_trace = __shatter_trace[:0]
	__shatter_mu.Unlock()
}

// __shatter_collect_results returns the current recorded results without
// clearing state. Callers should invoke __shatter_reset() before the next
// iteration so recordings do not accumulate across calls.
func __shatter_collect_results() __shatterResults {
	__shatter_mu.Lock()
	defer __shatter_mu.Unlock()
	results := __shatterResults{
		LinesExecuted: __shatter_lines,
		BranchPath:    __shatter_branches,
		ScopeEvents:   __shatter_trace,
	}
	if results.LinesExecuted == nil {
		results.LinesExecuted = []int{}
	}
	if results.BranchPath == nil {
		results.BranchPath = []__shatterBranchDecision{}
	}
	if results.ScopeEvents == nil {
		results.ScopeEvents = []__shatterTraceEvent{}
	}
	return results
}

// __shatter_get_results serializes the current recorded results to JSON bytes
// without clearing state. Useful for sending results over stdio.
func __shatter_get_results() ([]byte, error) {
	__shatter_mu.Lock()
	results := __shatterResults{
		LinesExecuted: __shatter_lines,
		BranchPath:    __shatter_branches,
		ScopeEvents:   __shatter_trace,
	}
	if results.LinesExecuted == nil {
		results.LinesExecuted = []int{}
	}
	if results.BranchPath == nil {
		results.BranchPath = []__shatterBranchDecision{}
	}
	if results.ScopeEvents == nil {
		results.ScopeEvents = []__shatterTraceEvent{}
	}
	__shatter_mu.Unlock()
	return json.Marshal(results)
}
`, packageName)
}
