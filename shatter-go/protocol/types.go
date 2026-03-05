// Package protocol defines the Shatter frontend protocol types.
//
// These types match the JSON schemas in protocol/schemas/ and the Rust
// types in shatter-core/src/. The protocol uses newline-delimited JSON
// (NDJSON) over stdin/stdout between the core engine and language frontends.
package protocol

import (
	"encoding/json"
	"fmt"
)

const ProtocolVersion = "0.1.0"

// Request is a message from the core engine to the frontend.
type Request struct {
	ProtocolVersion string `json:"protocol_version"`
	ID              int    `json:"id"`
	Command         string `json:"command"`

	// Handshake fields
	Capabilities []string `json:"capabilities,omitempty"`

	// Analyze fields
	File        string  `json:"file,omitempty"`
	Function    *string `json:"function,omitempty"`
	ProjectRoot *string `json:"project_root,omitempty"`

	// Instrument/Execute fields
	Mocks []MockConfig `json:"mocks,omitempty"`

	// Execute fields
	Inputs       []json.RawMessage `json:"inputs,omitempty"`
	SetupContext *json.RawMessage  `json:"setup_context,omitempty"`

	// Setup fields
	Mode string `json:"mode,omitempty"`

	// Generate fields
	Name   string           `json:"name,omitempty"`
	Kind   string           `json:"kind,omitempty"`
	Recipe *json.RawMessage `json:"recipe,omitempty"`
}

// Response is a message from the frontend to the core engine.
// Only fields relevant to the given status are populated.
type Response struct {
	ProtocolVersion string `json:"protocol_version"`
	ID              int    `json:"id"`
	Status          string `json:"status"`

	// Handshake
	FrontendVersion string   `json:"frontend_version,omitempty"`
	Language        string   `json:"language,omitempty"`
	Capabilities    []string `json:"capabilities,omitempty"`

	// Analyze
	Functions []FunctionAnalysis `json:"functions"`

	// Instrument
	Instrumented *bool   `json:"instrumented,omitempty"`
	OutputFile   *string `json:"output_file,omitempty"`

	// Execute
	ReturnValue       json.RawMessage   `json:"return_value,omitempty"`
	ThrownError       *ErrorInfo        `json:"thrown_error,omitempty"`
	BranchPath        []BranchDecision  `json:"branch_path,omitempty"`
	LinesExecuted     []int             `json:"lines_executed,omitempty"`
	CallsToExternal   []ExternalCall    `json:"calls_to_external,omitempty"`
	PathConstraints   []SymConstraint   `json:"path_constraints,omitempty"`
	SideEffects       []SideEffect      `json:"side_effects,omitempty"`
	ScopeEvents       []json.RawMessage `json:"scope_events,omitempty"`
	CaptureTruncation *TruncationInfo   `json:"capture_truncation,omitempty"`
	Performance       *PerfMetrics      `json:"performance,omitempty"`

	// Setup
	SetupContext *json.RawMessage `json:"setup_context,omitempty"`

	// Generate
	Value       *json.RawMessage `json:"value,omitempty"`
	GeneratorID string           `json:"generator_id,omitempty"`
	Recipe      *json.RawMessage `json:"recipe,omitempty"`

	// Error
	Code    string           `json:"code,omitempty"`
	Message string           `json:"message,omitempty"`
	Details *json.RawMessage `json:"details,omitempty"`
}

// ObjectField is a name-type pair for object fields.
// It serializes as a JSON 2-element array: ["fieldName", {type}].
type ObjectField struct {
	Name string
	Type TypeInfo
}

func (f ObjectField) MarshalJSON() ([]byte, error) {
	return json.Marshal([2]any{f.Name, f.Type})
}

func (f *ObjectField) UnmarshalJSON(data []byte) error {
	var raw [2]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		return fmt.Errorf("object field must be a 2-element array: %w", err)
	}
	if err := json.Unmarshal(raw[0], &f.Name); err != nil {
		return fmt.Errorf("object field name: %w", err)
	}
	if err := json.Unmarshal(raw[1], &f.Type); err != nil {
		return fmt.Errorf("object field type: %w", err)
	}
	return nil
}

// TypeInfo represents a type in the Shatter type system.
type TypeInfo struct {
	Kind        string                 `json:"kind"`
	Label       string                 `json:"label,omitempty"`        // opaque
	Element     *TypeInfo              `json:"element,omitempty"`      // array
	Fields      []ObjectField          `json:"fields,omitempty"`       // object
	Variants    []TypeInfo             `json:"variants,omitempty"`     // union
	Inner       *TypeInfo              `json:"inner,omitempty"`        // nullable, complex wrapper
	ComplexKind string                 `json:"complex_kind,omitempty"` // complex
	Metadata    map[string]interface{} `json:"metadata,omitempty"`     // complex
}

// ParamInfo describes a function parameter.
type ParamInfo struct {
	Name     string   `json:"name"`
	Type     TypeInfo `json:"type"`
	TypeName *string  `json:"type_name,omitempty"`
}

// LiteralValue is a literal constant found in a function body, used as a
// candidate test input. The JSON tags match the Rust core's
// #[serde(tag = "type", rename_all = "snake_case")] enum encoding.
type LiteralValue struct {
	Type    string `json:"type"`
	Value   any    `json:"value,omitempty"`   // int/float/str/bool literals
	Pattern string `json:"pattern,omitempty"` // regex literals only
}

// FunctionAnalysis is the result of analyzing a single function.
type FunctionAnalysis struct {
	Name         string               `json:"name"`
	Exported     bool                 `json:"exported,omitempty"`
	Params       []ParamInfo          `json:"params"`
	Branches     []BranchInfo         `json:"branches"`
	Dependencies []ExternalDependency `json:"dependencies"`
	ReturnType   TypeInfo             `json:"return_type"`
	StartLine    int                  `json:"start_line"`
	EndLine      int                  `json:"end_line"`
	Literals     []LiteralValue       `json:"literals,omitempty"`
}

// BranchInfo describes a branch point in the source code.
type BranchInfo struct {
	ID            int      `json:"id"`
	Line          int      `json:"line"`
	ConditionText string   `json:"condition_text"`
	Condition     *SymExpr `json:"condition"`
	BranchType    string   `json:"branch_type"`
}

// BranchDecision records which way a branch was taken during execution.
type BranchDecision struct {
	BranchID   int            `json:"branch_id"`
	Line       int            `json:"line"`
	Taken      bool           `json:"taken"`
	Constraint *SymConstraint `json:"constraint"`
}

// SymExpr is a symbolic expression representing a constraint on inputs.
type SymExpr struct {
	Kind     string    `json:"kind"`
	Name     string    `json:"name,omitempty"`
	Path     []string  `json:"path"`
	Type     string    `json:"type,omitempty"`
	Value    any       `json:"value,omitempty"`
	Op       string    `json:"op,omitempty"`
	Left     *SymExpr  `json:"left,omitempty"`
	Right    *SymExpr  `json:"right,omitempty"`
	Operand  *SymExpr  `json:"operand,omitempty"`
	Receiver *SymExpr  `json:"receiver,omitempty"`
	Args     []SymExpr `json:"args,omitempty"`
}

// SymConstraint is either an expression constraint or an unknown hint.
type SymConstraint struct {
	Kind string   `json:"kind"`
	Expr *SymExpr `json:"expr,omitempty"`
	Hint string   `json:"hint,omitempty"`
}

// ExternalDependency describes a dependency on an external symbol.
type ExternalDependency struct {
	Kind         string     `json:"kind"`
	Symbol       string     `json:"symbol"`
	SourceModule string     `json:"source_module"`
	ReturnType   TypeInfo   `json:"return_type"`
	ParamTypes   []TypeInfo `json:"param_types"`
	CallSites    []int      `json:"call_sites"`
}

// ExternalCall records an observed call to an external dependency.
type ExternalCall struct {
	Symbol      string `json:"symbol"`
	Args        []any  `json:"args"`
	ReturnValue any    `json:"return_value"`
}

// MockConfig specifies how to mock an external dependency.
type MockConfig struct {
	Symbol           string `json:"symbol"`
	ReturnValues     []any  `json:"return_values"`
	ShouldTrackCalls bool   `json:"should_track_calls"`
	DefaultBehavior  string `json:"default_behavior"`
}

// ErrorInfo describes an error thrown during execution.
type ErrorInfo struct {
	ErrorType     string  `json:"error_type"`
	Message       string  `json:"message"`
	Stack         *string `json:"stack"`
	ErrorCategory *string `json:"error_category,omitempty"`
}

// TruncationInfo contains metadata about truncation applied to captured side effects.
type TruncationInfo struct {
	WasTruncated  bool   `json:"was_truncated"`
	OriginalLines uint32 `json:"original_lines"`
	OriginalBytes uint64 `json:"original_bytes"`
}

// SideEffect represents an observed side effect.
type SideEffect struct {
	Kind      string           `json:"kind"`
	Level     string           `json:"level,omitempty"`
	Message   string           `json:"message,omitempty"`
	Path      string           `json:"path,omitempty"`
	Content   string           `json:"content,omitempty"`
	Method    string           `json:"method,omitempty"`
	URL       string           `json:"url,omitempty"`
	Body      *json.RawMessage `json:"body,omitempty"`
	Name      string           `json:"name,omitempty"`
	ErrorType string           `json:"error_type,omitempty"`
	Stack     *string          `json:"stack,omitempty"`
	Variable  string           `json:"variable,omitempty"`
	Value     *string          `json:"value,omitempty"`
	Before    *json.RawMessage `json:"before,omitempty"`
	After     *json.RawMessage `json:"after,omitempty"`
}

// PerfMetrics captures execution performance data.
type PerfMetrics struct {
	WallTimeMs         float64 `json:"wall_time_ms"`
	CPUTimeUs          int     `json:"cpu_time_us"`
	HeapUsedBytes      int     `json:"heap_used_bytes"`
	HeapAllocatedBytes int     `json:"heap_allocated_bytes"`
}
