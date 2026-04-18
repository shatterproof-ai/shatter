// Package protocol defines the Shatter frontend protocol types.
//
// These types match the JSON schemas in protocol/schemas/ and the Rust
// types in shatter-core/src/. The protocol uses newline-delimited JSON
// (NDJSON) over stdin/stdout between the core engine and language frontends.
package protocol

import (
	"encoding/json"
	"fmt"

	frontendtiming "github.com/shatter-dev/shatter/shatter-go/timing"
)

const ProtocolVersion = "0.1.0"

// SetupLevel defines the lifecycle granularity for setup/teardown.
// Values match the Rust core's SetupLevel enum (snake_case serialization).
type SetupLevel string

const (
	SetupLevelSession   SetupLevel = "session"
	SetupLevelFile      SetupLevel = "file"
	SetupLevelFunction  SetupLevel = "function"
	SetupLevelExecution SetupLevel = "execution"
)

// ValidSetupLevels lists all valid setup levels for validation.
var ValidSetupLevels = []SetupLevel{
	SetupLevelSession, SetupLevelFile, SetupLevelFunction, SetupLevelExecution,
}

// IsValid returns true if the level is a recognized setup level.
func (l SetupLevel) IsValid() bool {
	for _, v := range ValidSetupLevels {
		if l == v {
			return true
		}
	}
	return false
}

// SetupContextEntry associates a lifecycle level with the opaque context
// value returned by its Setup command.
type SetupContextEntry struct {
	Level   SetupLevel       `json:"level"`
	Context *json.RawMessage `json:"context"`
}

// SetupContextStack is a stack of active setup contexts, ordered from
// outermost (session) to innermost (execution). Passed to Execute so
// frontends can restore all active setup state.
type SetupContextStack struct {
	Contexts []SetupContextEntry `json:"contexts"`
}

// ExecutionAdapterApply defines the application policy for an execution adapter.
type ExecutionAdapterApply string

const (
	ExecutionAdapterApplyRequired ExecutionAdapterApply = "required"
	ExecutionAdapterApplyAuto     ExecutionAdapterApply = "auto"
	ExecutionAdapterApplySuggest  ExecutionAdapterApply = "suggest"
	ExecutionAdapterApplyDisabled ExecutionAdapterApply = "disabled"
)

// ExecutionAdapter is an opaque adapter descriptor passed through to frontends.
type ExecutionAdapter struct {
	ID      string                 `json:"id"`
	Apply   *ExecutionAdapterApply `json:"apply,omitempty"`
	Options *json.RawMessage       `json:"options,omitempty"`
}

// ExecutionProfile is an ordered list of opaque execution adapter descriptors.
type ExecutionProfile struct {
	Adapters []ExecutionAdapter `json:"adapters"`
}

// Request is a message from the core engine to the frontend.
type Request struct {
	ProtocolVersion string `json:"protocol_version"`
	ID              int    `json:"id"`
	Command         string `json:"command"`

	// Handshake fields
	Capabilities []string `json:"capabilities,omitempty"`

	// Analyze fields
	File             string            `json:"file,omitempty"`
	Function         *string           `json:"function,omitempty"`
	ProjectRoot      *string           `json:"project_root,omitempty"`
	ExecutionProfile *ExecutionProfile `json:"execution_profile,omitempty"`

	// Instrument/Execute fields
	Mocks []MockConfig `json:"mocks,omitempty"`

	// Execute fields
	Inputs       []json.RawMessage  `json:"inputs,omitempty"`
	PrepareID    *string            `json:"prepare_id,omitempty"`
	SetupContext *SetupContextStack `json:"setup_context,omitempty"`
	// Capture controls whether side effects (console output, file writes, etc.) are
	// collected. Nil or true means capture; false means skip for lower overhead.
	// Non-capture outputs (branch_path, lines_executed, return_value, thrown_error)
	// remain correct regardless of this setting.
	Capture *bool `json:"capture,omitempty"`

	// Setup/Teardown fields
	Scope         string             `json:"scope,omitempty"`
	Level         SetupLevel         `json:"level,omitempty"`
	ParentContext *SetupContextStack `json:"parent_context,omitempty"`

	// Generate fields
	Name   string           `json:"name,omitempty"`
	Kind   string           `json:"kind,omitempty"`
	Recipe *json.RawMessage `json:"recipe,omitempty"`
}

// Response is a message from the frontend to the core engine.
// Only fields relevant to the given status are populated.
type Response struct {
	ProtocolVersion string         `json:"protocol_version"`
	ID              int            `json:"id"`
	Status          string         `json:"status"`
	Timing          *TimingSummary `json:"timing,omitempty"`

	// Handshake
	FrontendVersion string   `json:"frontend_version,omitempty"`
	Language        string   `json:"language,omitempty"`
	Capabilities    []string `json:"capabilities,omitempty"`

	// Analyze
	Functions []FunctionAnalysis `json:"functions"`

	// Instrument
	Instrumented            *bool   `json:"instrumented,omitempty"`
	OutputFile              *string `json:"output_file,omitempty"`
	InstrumentableLineCount *int    `json:"instrumentable_line_count,omitempty"`

	// Execute
	ReturnValue            json.RawMessage        `json:"return_value,omitempty"`
	ThrownError            *ErrorInfo             `json:"thrown_error,omitempty"`
	BranchPath             []BranchDecision       `json:"branch_path,omitempty"`
	LinesExecuted          []int                  `json:"lines_executed,omitempty"`
	CallsToExternal        []ExternalCall         `json:"calls_to_external,omitempty"`
	DiscoveredDependencies []DiscoveredDependency `json:"discovered_dependencies,omitempty"`
	PathConstraints        []SymConstraint        `json:"path_constraints,omitempty"`
	SideEffects            []SideEffect           `json:"side_effects,omitempty"`
	ScopeEvents            []json.RawMessage      `json:"scope_events,omitempty"`
	LoopBodyStates         []LoopBodyState        `json:"loop_body_states,omitempty"`
	CaptureTruncation      *TruncationInfo        `json:"capture_truncation,omitempty"`
	Performance            *PerfMetrics           `json:"performance,omitempty"`

	// Prepare
	PrepareID string `json:"prepare_id,omitempty"`

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
	Kind          string                 `json:"kind"`
	Label         string                 `json:"label,omitempty"`          // opaque
	StaticOpacity string                 `json:"static_opacity,omitempty"` // static analysis opacity reason
	MediumOpacity string                 `json:"medium_opacity,omitempty"` // medium-confidence opacity signal
	Element       *TypeInfo              `json:"element,omitempty"`        // array
	Fields        []ObjectField          `json:"fields,omitempty"`         // object
	Variants      []TypeInfo             `json:"variants,omitempty"`       // union
	Inner         *TypeInfo              `json:"inner,omitempty"`          // nullable, complex wrapper
	ComplexKind   string                 `json:"complex_kind,omitempty"`   // complex
	Metadata      map[string]interface{} `json:"metadata,omitempty"`       // complex
}

// MarshalJSON ensures that object-kind TypeInfo always emits "fields" as []
// instead of omitting it when the slice is nil/empty. The Rust deserializer
// requires the key for the Object variant.
func (ti TypeInfo) MarshalJSON() ([]byte, error) {
	// For object kind, ensure Fields is non-nil so it serializes as [].
	// We use a struct without omitempty on Fields for this case.
	if ti.Kind == "object" {
		if ti.Fields == nil {
			ti.Fields = []ObjectField{}
		}
		type objectTypeInfo struct {
			Kind          string                 `json:"kind"`
			Label         string                 `json:"label,omitempty"`
			StaticOpacity string                 `json:"static_opacity,omitempty"`
			MediumOpacity string                 `json:"medium_opacity,omitempty"`
			Element       *TypeInfo              `json:"element,omitempty"`
			Fields        []ObjectField          `json:"fields"`
			Variants      []TypeInfo             `json:"variants,omitempty"`
			Inner         *TypeInfo              `json:"inner,omitempty"`
			ComplexKind   string                 `json:"complex_kind,omitempty"`
			Metadata      map[string]interface{} `json:"metadata,omitempty"`
		}
		return json.Marshal(objectTypeInfo{
			Kind:          ti.Kind,
			Label:         ti.Label,
			StaticOpacity: ti.StaticOpacity,
			MediumOpacity: ti.MediumOpacity,
			Element:       ti.Element,
			Fields:        ti.Fields,
			Variants:      ti.Variants,
			Inner:         ti.Inner,
			ComplexKind:   ti.ComplexKind,
			Metadata:      ti.Metadata,
		})
	}
	// Non-object kinds: use default serialization with omitempty on Fields.
	type typeInfoAlias TypeInfo
	return json.Marshal(typeInfoAlias(ti))
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

// CryptoBoundary represents a detected cryptographic API boundary within a function.
// Populated by core after analysis; frontends leave this empty.
type CryptoBoundary struct {
	Symbol        string            `json:"symbol"`
	SourceModule  string            `json:"source_module"`
	Direction     string            `json:"direction"`
	Output        string            `json:"output,omitempty"`
	Confidence    string            `json:"confidence,omitempty"`
	ParamRoles    map[string]string `json:"param_roles,omitempty"`
	CallSites     []int             `json:"call_sites"`
	InputEntropy  *float64          `json:"input_entropy,omitempty"`
	OutputEntropy *float64          `json:"output_entropy,omitempty"`
}

// FunctionAnalysis is the result of analyzing a single function.
type FunctionAnalysis struct {
	Name             string               `json:"name"`
	Exported         bool                 `json:"exported,omitempty"`
	Params           []ParamInfo          `json:"params"`
	Branches         []BranchInfo         `json:"branches"`
	Dependencies     []ExternalDependency `json:"dependencies"`
	ReturnType       TypeInfo             `json:"return_type"`
	StartLine        int                  `json:"start_line"`
	EndLine          int                  `json:"end_line"`
	Literals         []LiteralValue       `json:"literals,omitempty"`
	CryptoBoundaries []CryptoBoundary     `json:"crypto_boundaries,omitempty"`
	Loops            []LoopInfo           `json:"loops,omitempty"`
	SourceFile       string               `json:"source_file,omitempty"`
	AdapterHints     []AdapterHint        `json:"adapter_hints,omitempty"`
	InvocationModel  *InvocationModel     `json:"invocation_model,omitempty"`
}

// InvocationModel describes how a discovered target should be invoked.
type InvocationModel struct {
	Kind            string           `json:"kind"`
	AdapterID       string           `json:"adapter_id,omitempty"`
	SyntheticParams []ParamInfo      `json:"synthetic_params,omitempty"`
	ScenarioSchema  *json.RawMessage `json:"scenario_schema,omitempty"`
}

// AdapterRelation links an adapter hint to another adapter.
type AdapterRelation struct {
	AdapterID string `json:"adapter_id"`
	Reason    string `json:"reason,omitempty"`
}

// AdapterHint is a recognizer-generated signal that a function is a handler target.
type AdapterHint struct {
	Adapter      ExecutionAdapter  `json:"adapter"`
	Confidence   string            `json:"confidence,omitempty"`
	Reasons      []string          `json:"reasons,omitempty"`
	Requirements []AdapterRelation `json:"requirements,omitempty"`
	Conflicts    []AdapterRelation `json:"conflicts,omitempty"`
}

// BranchInfo describes a branch point in the source code.
type BranchInfo struct {
	ID            int      `json:"id"`
	Line          int      `json:"line"`
	ConditionText string   `json:"condition_text"`
	Condition     *SymExpr `json:"condition"`
	BranchType    string   `json:"branch_type"`
}

// InductionVar holds metadata about a loop induction variable detected during
// static analysis of a canonical counted for-loop.
type InductionVar struct {
	Name      string   `json:"name"`
	InitExpr  *SymExpr `json:"init_expr"`
	StepExpr  *SymExpr `json:"step_expr"`
	BoundExpr *SymExpr `json:"bound_expr"`
	BoundOp   string   `json:"bound_op"` // "lt", "le", "gt", "ge"
}

// LoopInfo represents a canonical counted loop detected during static analysis.
// Only loops where the induction variable can be fully characterized are included.
type LoopInfo struct {
	LoopID       int           `json:"loop_id"`
	Line         int           `json:"line"`
	InductionVar *InductionVar `json:"induction_var"`
}

// ConditionOutcome records the outcome of an individual condition within a compound decision.
type ConditionOutcome struct {
	ConditionIndex int            `json:"condition_index"`
	Value          *bool          `json:"value"` // nil if masked by short-circuit
	Masked         bool           `json:"masked,omitempty"`
	Constraint     *SymConstraint `json:"constraint"`
}

// BranchDecision records which way a branch was taken during execution.
type BranchDecision struct {
	BranchID   int            `json:"branch_id"`
	Line       int            `json:"line"`
	Taken      bool           `json:"taken"`
	Constraint *SymConstraint `json:"constraint"`
	// Conditions holds per-condition outcomes for MC/DC analysis.
	// Present only when MC/DC mode is enabled and the decision is compound.
	Conditions []ConditionOutcome `json:"conditions,omitempty"`
}

type LoopBodyState struct {
	LoopID    int                `json:"loop_id"`
	Iteration int                `json:"iteration"`
	Locals    map[string]SymExpr `json:"locals,omitempty"`
}

// SymExpr is a symbolic expression representing a constraint on inputs.
type SymExpr struct {
	Kind      string    `json:"kind"`
	Name      string    `json:"name,omitempty"`
	Path      []string  `json:"path"`
	Type      string    `json:"type,omitempty"`
	Value     any       `json:"value,omitempty"`
	Op        string    `json:"op,omitempty"`
	Left      *SymExpr  `json:"left,omitempty"`
	Right     *SymExpr  `json:"right,omitempty"`
	Operand   *SymExpr  `json:"operand,omitempty"`
	Receiver  *SymExpr  `json:"receiver,omitempty"`
	Args      []SymExpr `json:"args"`
	Condition *SymExpr  `json:"condition,omitempty"`
	ThenExpr  *SymExpr  `json:"then_expr,omitempty"`
	ElseExpr  *SymExpr  `json:"else_expr,omitempty"`
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

// DiscoveredDependency represents a dependency found at execution time
// that was not covered by the provided mocks.
type DiscoveredDependency struct {
	Symbol            string `json:"symbol"`
	SourceModule      string `json:"source_module"`
	Kind              string `json:"kind"` // "unmocked_import" or "subprocess_spawn"
	IsSubprocessSpawn bool   `json:"is_subprocess_spawn"`
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

// OutcomeStatus classifies the result of a single invocation attempt.
type OutcomeStatus string

const (
	OutcomeStatusCompleted             OutcomeStatus = "completed"
	OutcomeStatusCompletedWithFindings OutcomeStatus = "completed_with_findings"
	OutcomeStatusUnsupported           OutcomeStatus = "unsupported"
	OutcomeStatusBuildFailed           OutcomeStatus = "build_failed"
	OutcomeStatusRuntimeFailed         OutcomeStatus = "runtime_failed"
	OutcomeStatusTimedOut              OutcomeStatus = "timed_out"
	OutcomeStatusSkippedByPolicy       OutcomeStatus = "skipped_by_policy"
)

// InvocationOutcome is the reusable protocol contract for one invocation result.
type InvocationOutcome struct {
	Status      OutcomeStatus   `json:"status"`
	ReturnValue json.RawMessage `json:"return_value,omitempty"`
	ThrownError *ErrorInfo      `json:"thrown_error,omitempty"`
	SideEffects []SideEffect    `json:"side_effects,omitempty"`
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

// TimingSummary captures aggregated phase timings for a frontend command.
type TimingSummary = frontendtiming.Summary

// TimingPhaseSummary captures one named timing phase.
type TimingPhaseSummary = frontendtiming.PhaseSummary
