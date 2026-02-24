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

type __shatterResults struct {
	LinesExecuted []int                     `+"`"+`json:"lines_executed"`+"`"+`
	BranchPath    []__shatterBranchDecision `+"`"+`json:"branch_path"`+"`"+`
}

var (
	__shatter_mu       sync.Mutex
	__shatter_lines    []int
	__shatter_branches []__shatterBranchDecision
)

func __shatter_record_line(line int) {
	__shatter_mu.Lock()
	__shatter_lines = append(__shatter_lines, line)
	__shatter_mu.Unlock()
}

func __shatter_record_branch(branchID, line int, cond bool, constraintJSON string) bool {
	__shatter_mu.Lock()
	__shatter_branches = append(__shatter_branches, __shatterBranchDecision{
		BranchID:       branchID,
		Line:           line,
		Taken:          cond,
		ConstraintJSON: constraintJSON,
	})
	__shatter_mu.Unlock()
	return cond
}

func __shatter_dump_results(path string) error {
	__shatter_mu.Lock()
	results := __shatterResults{
		LinesExecuted: __shatter_lines,
		BranchPath:    __shatter_branches,
	}
	if results.LinesExecuted == nil {
		results.LinesExecuted = []int{}
	}
	if results.BranchPath == nil {
		results.BranchPath = []__shatterBranchDecision{}
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
