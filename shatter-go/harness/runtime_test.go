package harness

import (
	"encoding/json"
	"testing"
)

func TestSafeCallReturnsNilOnSuccess(t *testing.T) {
	result := SafeCall(func() {
		_ = 1 + 1
	})
	if result != nil {
		t.Errorf("expected nil, got %+v", result)
	}
}

func TestSafeCallCapturesPanic(t *testing.T) {
	result := SafeCall(func() {
		panic("test panic")
	})
	if result == nil {
		t.Fatal("expected non-nil error")
	}
	if result.ErrorType != "panic" {
		t.Errorf("error_type = %q, want %q", result.ErrorType, "panic")
	}
	if result.Message != "test panic" {
		t.Errorf("message = %q, want %q", result.Message, "test panic")
	}
	if result.ErrorCategory != "runtime" {
		t.Errorf("error_category = %q, want %q", result.ErrorCategory, "runtime")
	}
	if result.Stack == "" {
		t.Error("expected non-empty stack trace")
	}
}

func TestSafeCallCapturesNonStringPanic(t *testing.T) {
	result := SafeCall(func() {
		panic(42)
	})
	if result == nil {
		t.Fatal("expected non-nil error")
	}
	if result.Message != "42" {
		t.Errorf("message = %q, want %q", result.Message, "42")
	}
}

func TestConsoleSideEffectsEmpty(t *testing.T) {
	se := ConsoleSideEffects("", "")
	if len(se) != 0 {
		t.Errorf("expected 0 side effects, got %d", len(se))
	}
}

func TestConsoleSideEffectsWhitespaceOnly(t *testing.T) {
	se := ConsoleSideEffects("  \n  ", "\t")
	if len(se) != 0 {
		t.Errorf("expected 0 side effects for whitespace-only input, got %d", len(se))
	}
}

func TestConsoleSideEffectsStdout(t *testing.T) {
	se := ConsoleSideEffects("hello world", "")
	if len(se) != 1 {
		t.Fatalf("expected 1 side effect, got %d", len(se))
	}
	if se[0].Kind != "console_output" {
		t.Errorf("kind = %q, want %q", se[0].Kind, "console_output")
	}
	if se[0].Level != "log" {
		t.Errorf("level = %q, want %q", se[0].Level, "log")
	}
	if se[0].Message != "hello world" {
		t.Errorf("message = %q, want %q", se[0].Message, "hello world")
	}
}

func TestConsoleSideEffectsStderr(t *testing.T) {
	se := ConsoleSideEffects("", "error output")
	if len(se) != 1 {
		t.Fatalf("expected 1 side effect, got %d", len(se))
	}
	if se[0].Level != "error" {
		t.Errorf("level = %q, want %q", se[0].Level, "error")
	}
}

func TestConsoleSideEffectsBoth(t *testing.T) {
	se := ConsoleSideEffects("stdout", "stderr")
	if len(se) != 2 {
		t.Fatalf("expected 2 side effects, got %d", len(se))
	}
	if se[0].Level != "log" {
		t.Errorf("first level = %q, want %q", se[0].Level, "log")
	}
	if se[1].Level != "error" {
		t.Errorf("second level = %q, want %q", se[1].Level, "error")
	}
}

func TestPerfStartFinish(t *testing.T) {
	snap := StartPerf()
	// Do some work to ensure non-zero measurements.
	data := make([]byte, 1024)
	_ = data
	perf := snap.Finish()

	if perf == nil {
		t.Fatal("expected non-nil perf")
	}
	if perf.WallTimeMs < 0 {
		t.Errorf("wall_time_ms should be non-negative, got %f", perf.WallTimeMs)
	}
	// CPU time should be non-negative (may be 0 on very fast runs).
	if perf.CPUTimeUs < 0 {
		t.Errorf("cpu_time_us should be non-negative, got %d", perf.CPUTimeUs)
	}
}

func TestResponseJSONRoundtrip(t *testing.T) {
	resp := Response{
		ReturnValue:   json.RawMessage(`42`),
		BranchPath:    json.RawMessage(`[{"branch_id":1,"line":10,"taken":true}]`),
		LinesExecuted: json.RawMessage(`[1,2,3]`),
		ScopeEvents:   json.RawMessage(`[]`),
		SideEffects: []SideEffect{
			{Kind: "console_output", Level: "log", Message: "hello"},
		},
		Performance: &Perf{WallTimeMs: 1.5, CPUTimeUs: 1500},
	}

	data, err := json.Marshal(resp)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}

	var decoded Response
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}

	if string(decoded.ReturnValue) != "42" {
		t.Errorf("return_value = %s, want 42", decoded.ReturnValue)
	}
	if len(decoded.SideEffects) != 1 {
		t.Errorf("side_effects count = %d, want 1", len(decoded.SideEffects))
	}
	if decoded.Performance.WallTimeMs != 1.5 {
		t.Errorf("wall_time_ms = %f, want 1.5", decoded.Performance.WallTimeMs)
	}
}

func TestResponseOmitsEmptyFields(t *testing.T) {
	resp := Response{
		BranchPath:    json.RawMessage(`[]`),
		LinesExecuted: json.RawMessage(`[]`),
		ScopeEvents:   json.RawMessage(`[]`),
		Performance:   &Perf{},
	}

	data, err := json.Marshal(resp)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}

	var raw map[string]json.RawMessage
	if err := json.Unmarshal(data, &raw); err != nil {
		t.Fatalf("unmarshal raw: %v", err)
	}

	// return_value should be omitted when nil
	if _, ok := raw["return_value"]; ok {
		t.Error("return_value should be omitted when nil")
	}
	// thrown_error should be omitted when nil
	if _, ok := raw["thrown_error"]; ok {
		t.Error("thrown_error should be omitted when nil")
	}
	// external_calls should be omitted when nil
	if _, ok := raw["external_calls"]; ok {
		t.Error("external_calls should be omitted when nil")
	}
	// error should be omitted when empty
	if _, ok := raw["error"]; ok {
		t.Error("error should be omitted when empty string")
	}
}

func TestErrorJSON(t *testing.T) {
	e := Error{
		ErrorType:     "panic",
		Message:       "boom",
		Stack:         "goroutine 1 [running]:\nmain.main()",
		ErrorCategory: "runtime",
	}
	data, err := json.Marshal(e)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}

	var decoded Error
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if decoded != e {
		t.Errorf("roundtrip mismatch: got %+v, want %+v", decoded, e)
	}
}

func TestSideEffectJSON(t *testing.T) {
	se := SideEffect{
		Kind:     "global_state_change",
		Variable: "Counter",
		Before:   json.RawMessage(`0`),
		After:    json.RawMessage(`5`),
	}
	data, err := json.Marshal(se)
	if err != nil {
		t.Fatalf("marshal: %v", err)
	}

	var decoded SideEffect
	if err := json.Unmarshal(data, &decoded); err != nil {
		t.Fatalf("unmarshal: %v", err)
	}
	if decoded.Kind != "global_state_change" {
		t.Errorf("kind = %q, want %q", decoded.Kind, "global_state_change")
	}
	if decoded.Variable != "Counter" {
		t.Errorf("variable = %q, want %q", decoded.Variable, "Counter")
	}
}
