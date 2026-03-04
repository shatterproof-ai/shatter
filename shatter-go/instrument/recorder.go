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

type __shatterBranchDecision struct {
	BranchID       int    `+"`"+`json:"branch_id"`+"`"+`
	Line           int    `+"`"+`json:"line"`+"`"+`
	Taken          bool   `+"`"+`json:"taken"`+"`"+`
	ConstraintJSON string `+"`"+`json:"constraint_json,omitempty"`+"`"+`
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
`, packageName)
}
