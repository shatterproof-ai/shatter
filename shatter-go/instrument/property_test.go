package instrument

import (
	"encoding/json"
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"regexp"
	"strconv"
	"strings"
	"testing"
	"time"
	"unicode/utf8"

	"pgregory.net/rapid"
)

var validIdentChar = regexp.MustCompile(`^[a-zA-Z0-9_]*$`)

// ---------------------------------------------------------------------------
// Semantic properties — sanitizeMockName
// ---------------------------------------------------------------------------

func TestPropertySanitizeMockNameIdempotent(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		input := rapid.String().Draw(t, "input")
		once := sanitizeMockName(input)
		twice := sanitizeMockName(once)
		if once != twice {
			t.Fatalf("not idempotent: sanitize(%q)=%q, sanitize(%q)=%q", input, once, once, twice)
		}
	})
}

func TestPropertySanitizeMockNameValidChars(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		input := rapid.String().Draw(t, "input")
		result := sanitizeMockName(input)
		if !validIdentChar.MatchString(result) {
			t.Fatalf("invalid chars in sanitize(%q) = %q", input, result)
		}
	})
}

func TestPropertySanitizeMockNamePreservesRuneCount(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		input := rapid.String().Draw(t, "input")
		result := sanitizeMockName(input)
		// Each rune in input maps to exactly one byte in output
		if len(result) != utf8.RuneCountInString(input) {
			t.Fatalf("length mismatch: rune count of %q is %d, but sanitize produced %d chars: %q",
				input, utf8.RuneCountInString(input), len(result), result)
		}
	})
}

// ---------------------------------------------------------------------------
// Semantic properties — timeout contract
// ---------------------------------------------------------------------------

func TestPropertyTimeoutBoundedByMax(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		val := rapid.OneOf(
			rapid.StringMatching(`[0-9]{1,10}(\.[0-9]{1,3})?`),
			rapid.Just(""),
			rapid.Just("999999999"),
		).Draw(t, "envVal")

		maxDur := time.Duration(maxTimeoutSecs) * time.Second

		os.Setenv("SHATTER_EXEC_TIMEOUT", val)
		defer os.Unsetenv("SHATTER_EXEC_TIMEOUT")
		dur := execTimeout()
		if dur > maxDur {
			t.Fatalf("execTimeout()=%v exceeds max %v for input %q", dur, maxDur, val)
		}
	})
}

// ---------------------------------------------------------------------------
// Roundtrip properties — timeout (existing)
// ---------------------------------------------------------------------------

func TestPropertyExecTimeoutAlwaysPositive(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		val := rapid.OneOf(
			rapid.StringMatching(`[0-9]{1,5}(\.[0-9]{1,3})?`),
			rapid.StringMatching(`[a-zA-Z ]{0,10}`),
			rapid.Just(""),
			rapid.Just("0"),
			rapid.Just("-1"),
			rapid.Just("NaN"),
			rapid.Just("inf"),
		).Draw(t, "envVal")

		os.Setenv("SHATTER_EXEC_TIMEOUT", val)
		defer os.Unsetenv("SHATTER_EXEC_TIMEOUT")
		dur := execTimeout()

		if dur <= 0 {
			t.Fatalf("execTimeout() returned non-positive %v for input %q", dur, val)
		}
	})
}

func TestPropertyExecTimeoutValidNumbersApplied(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		secs := rapid.Float64Range(0.01, 3600).Draw(t, "secs")
		val := fmt.Sprintf("%g", secs)

		os.Setenv("SHATTER_EXEC_TIMEOUT", val)
		defer os.Unsetenv("SHATTER_EXEC_TIMEOUT")
		dur := execTimeout()

		parsed, err := strconv.ParseFloat(val, 64)
		if err == nil && parsed > 0 {
			expected := time.Duration(parsed * float64(time.Second))
			if dur != expected {
				t.Fatalf("execTimeout()=%v, want %v for input %q", dur, expected, val)
			}
		}
	})
}

// ---------------------------------------------------------------------------
// Semantic properties — throw_error mock generation
// ---------------------------------------------------------------------------

func TestPropertyThrowErrorMockContainsPanic(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		symbol := rapid.StringMatching(`[a-zA-Z][a-zA-Z0-9_.:-]{0,30}`).Draw(t, "symbol")
		mocks := []MockConfig{
			{
				Symbol:          symbol,
				ReturnValues:    []any{map[string]any{"message": "err"}},
				DefaultBehavior: BehaviorThrowError,
			},
		}
		source := generateLoopMockFile(mocks)
		if !strings.Contains(source, "panic(msg)") {
			t.Fatalf("throw_error mock for %q missing panic(msg)", symbol)
		}
	})
}

// ---------------------------------------------------------------------------
// Semantic properties — discoverDependencies
// ---------------------------------------------------------------------------

func TestPropertyDiscoverDepsNeverIncludesMockedModules(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		modCount := rapid.IntRange(1, 5).Draw(t, "modCount")
		modules := make([]string, modCount)
		for i := range modules {
			modules[i] = fmt.Sprintf("github.com/org/mod%d", rapid.IntRange(0, 100).Draw(t, fmt.Sprintf("mod%d", i)))
		}

		// Build source with these imports
		var imports string
		for _, m := range modules {
			imports += fmt.Sprintf("\t%q\n", m)
		}
		src := fmt.Sprintf("package example\n\nimport (\n%s)\n\nfunc F() {}\n", imports)

		dir, err := os.MkdirTemp("", "shatter-prop-*")
		if err != nil {
			t.Fatalf("mkdirtemp: %v", err)
		}
		defer os.RemoveAll(dir)
		path := dir + "/test.go"
		os.WriteFile(path, []byte(src), 0644)

		// Mock all modules
		mocks := make([]MockConfig, len(modules))
		for i, m := range modules {
			mocks[i] = MockConfig{Symbol: m + ":Func"}
		}

		deps := discoverDependencies(path, mocks)
		if len(deps) != 0 {
			t.Fatalf("expected 0 deps when all modules mocked, got %d: %+v", len(deps), deps)
		}
	})
}

// genMockConfig generates a random MockConfig for property tests.
func genMockConfig(t *rapid.T) MockConfig {
	return MockConfig{
		Symbol: rapid.StringMatching(`[a-zA-Z][a-zA-Z0-9_.:-]{0,20}`).Draw(t, "symbol"),
		ReturnValues: func() []any {
			n := rapid.IntRange(0, 3).Draw(t, "nRetVals")
			vals := make([]any, n)
			for i := range vals {
				vals[i] = rapid.OneOf(
					rapid.Map(rapid.Int(), func(v int) any { return v }),
					rapid.Map(rapid.String(), func(v string) any { return v }),
				).Draw(t, fmt.Sprintf("retval%d", i))
			}
			return vals
		}(),
		ShouldTrackCalls: rapid.Bool().Draw(t, "trackCalls"),
		DefaultBehavior: rapid.SampledFrom([]string{
			BehaviorRepeatLast, BehaviorCycle, BehaviorThrowError, BehaviorPassthrough,
		}).Draw(t, "behavior"),
	}
}

func TestPropertyMockFileAlwaysStartsWithPackageMain(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		n := rapid.IntRange(1, 5).Draw(t, "nMocks")
		mocks := make([]MockConfig, n)
		for i := range mocks {
			mocks[i] = genMockConfig(t)
		}
		source := generateLoopMockFile(mocks)
		if !strings.Contains(source, "package main") {
			t.Fatal("generated mock file must start with package main")
		}
	})
}

func TestPropertyPassthroughProducesNoMockFunction(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		symbol := rapid.StringMatching(`[a-zA-Z][a-zA-Z0-9_.:-]{0,20}`).Draw(t, "symbol")
		mocks := []MockConfig{
			{
				Symbol:          symbol,
				DefaultBehavior: BehaviorPassthrough,
			},
		}
		source := generateLoopMockFile(mocks)
		safeName := sanitizeMockName(symbol)
		if strings.Contains(source, "ShatterMock_"+safeName) {
			t.Fatalf("passthrough mock for %q should not generate ShatterMock_ function", symbol)
		}
	})
}

func TestPropertyThrowErrorProducesBothVariants(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		symbol := rapid.StringMatching(`[a-zA-Z][a-zA-Z0-9_.:-]{0,20}`).Draw(t, "symbol")
		mocks := []MockConfig{
			{
				Symbol:          symbol,
				ReturnValues:    []any{map[string]any{"message": "err"}},
				DefaultBehavior: BehaviorThrowError,
			},
		}
		source := generateLoopMockFile(mocks)
		safeName := sanitizeMockName(symbol)

		if !strings.Contains(source, "func ShatterMock_"+safeName+"(args ...any) any") {
			t.Fatalf("missing panic variant for %q", symbol)
		}
		if !strings.Contains(source, "func ShatterMockErr_"+safeName+"(args ...any) (any, error)") {
			t.Fatalf("missing error-return variant for %q", symbol)
		}
	})
}

func TestPropertyCallTrackingConditional(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		symbol := rapid.StringMatching(`[a-zA-Z][a-zA-Z0-9_.]{0,15}`).Draw(t, "symbol")
		track := rapid.Bool().Draw(t, "track")
		behavior := rapid.SampledFrom([]string{
			BehaviorRepeatLast, BehaviorCycle, BehaviorThrowError,
		}).Draw(t, "behavior")
		mocks := []MockConfig{
			{
				Symbol:           symbol,
				ReturnValues:     []any{"val"},
				ShouldTrackCalls: track,
				DefaultBehavior:  behavior,
			},
		}
		source := generateLoopMockFile(mocks)
		hasTracking := strings.Contains(source, fmt.Sprintf(`shatterRecordMockCall(%q`, symbol))

		if track && !hasTracking {
			t.Fatalf("ShouldTrackCalls=true but no shatterRecordMockCall for %q (behavior=%s)", symbol, behavior)
		}
		if !track && hasTracking {
			t.Fatalf("ShouldTrackCalls=false but shatterRecordMockCall present for %q (behavior=%s)", symbol, behavior)
		}
	})
}

func TestPropertyExecTimeoutInvalidFallsBack(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		val := rapid.OneOf(
			rapid.Just(""),
			rapid.Just("abc"),
			rapid.Just("0"),
			rapid.Just("-5"),
			rapid.Just("  "),
		).Draw(t, "envVal")

		os.Setenv("SHATTER_EXEC_TIMEOUT", val)
		defer os.Unsetenv("SHATTER_EXEC_TIMEOUT")
		dur := execTimeout()

		if dur != defaultExecTimeout {
			t.Fatalf("execTimeout()=%v, want default %v for invalid input %q", dur, defaultExecTimeout, val)
		}
	})
}

// ---------------------------------------------------------------------------
// MC/DC mode detection
// ---------------------------------------------------------------------------

func TestPropertyIsMcdcEnabledOnlyForExactValue(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		val := rapid.OneOf(
			rapid.Just("1"),
			rapid.Just("0"),
			rapid.Just("true"),
			rapid.Just(""),
			rapid.Just("yes"),
			rapid.Just("ON"),
		).Draw(t, "val")

		os.Setenv("SHATTER_MCDC", val)
		defer os.Unsetenv("SHATTER_MCDC")

		enabled := isMcdcEnabled()
		if val == "1" && !enabled {
			t.Fatalf("isMcdcEnabled() should return true for SHATTER_MCDC=1")
		}
		if val != "1" && enabled {
			t.Fatalf("isMcdcEnabled() should return false for SHATTER_MCDC=%q", val)
		}
	})
}

// ---------------------------------------------------------------------------
// MC/DC short-circuit masking properties
//
// These tests verify the masking semantics defined in evalMcdcChain, which
// is the canonical implementation used by both the recorder and the tests.
// ---------------------------------------------------------------------------

// evalMcdcChain is the test-facing implementation of the short-circuit
// masking logic. It mirrors the __shatter_mcdc_record function that the
// recorder embeds in instrumented binaries.
//
// operator must be "and" or "or".
type mcdcOutcome struct {
	ConditionIndex int
	Value          *bool
	Masked         bool
}

func evalMcdcChain(operator string, vals []bool) (decision bool, outcomes []mcdcOutcome) {
	outcomes = make([]mcdcOutcome, len(vals))
	stopAfter := -1

	if operator == "and" {
		decision = true
		for i, v := range vals {
			if stopAfter >= 0 {
				outcomes[i] = mcdcOutcome{ConditionIndex: i, Masked: true}
				continue
			}
			v2 := v
			outcomes[i] = mcdcOutcome{ConditionIndex: i, Value: &v2}
			if !v {
				decision = false
				stopAfter = i
			}
		}
	} else {
		// "or"
		decision = false
		for i, v := range vals {
			if stopAfter >= 0 {
				outcomes[i] = mcdcOutcome{ConditionIndex: i, Masked: true}
				continue
			}
			v2 := v
			outcomes[i] = mcdcOutcome{ConditionIndex: i, Value: &v2}
			if v {
				decision = true
				stopAfter = i
			}
		}
	}
	return
}

// genBoolSlice generates a slice of 2-16 booleans for condition sequences.
func genBoolSlice(t *rapid.T) []bool {
	n := rapid.IntRange(2, 16).Draw(t, "n")
	vals := make([]bool, n)
	for i := range vals {
		vals[i] = rapid.Bool().Draw(t, fmt.Sprintf("cond%d", i))
	}
	return vals
}

// TestPropertyMcdcAndMaskingConsistency verifies that for && chains:
//   - conditions after the first false are masked
//   - the decision equals the AND of all non-masked conditions
func TestPropertyMcdcAndMaskingConsistency(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		vals := genBoolSlice(t)
		decision, outcomes := evalMcdcChain("and", vals)

		// Find expected stop point.
		stopAfter := -1
		for i, v := range vals {
			if !v {
				stopAfter = i
				break
			}
		}

		for i, outcome := range outcomes {
			if outcome.ConditionIndex != i {
				t.Fatalf("condition_index=%d, want %d", outcome.ConditionIndex, i)
			}
			if stopAfter >= 0 && i > stopAfter {
				if !outcome.Masked {
					t.Fatalf("condition %d should be masked (stopAfter=%d)", i, stopAfter)
				}
				if outcome.Value != nil {
					t.Fatalf("masked condition %d should have nil value", i)
				}
			} else {
				if outcome.Masked {
					t.Fatalf("condition %d should not be masked", i)
				}
				if outcome.Value == nil || *outcome.Value != vals[i] {
					t.Fatalf("condition %d: value mismatch", i)
				}
			}
		}

		// Verify decision consistency.
		expectedDecision := stopAfter < 0 // all true
		if decision != expectedDecision {
			t.Fatalf("decision=%v, want %v", decision, expectedDecision)
		}
	})
}

// TestPropertyMcdcOrMaskingConsistency verifies that for || chains:
//   - conditions after the first true are masked
//   - the decision equals the OR of all non-masked conditions
func TestPropertyMcdcOrMaskingConsistency(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		vals := genBoolSlice(t)
		decision, outcomes := evalMcdcChain("or", vals)

		// Find expected stop point.
		stopAfter := -1
		for i, v := range vals {
			if v {
				stopAfter = i
				break
			}
		}

		for i, outcome := range outcomes {
			if outcome.ConditionIndex != i {
				t.Fatalf("condition_index=%d, want %d", outcome.ConditionIndex, i)
			}
			if stopAfter >= 0 && i > stopAfter {
				if !outcome.Masked {
					t.Fatalf("condition %d should be masked (stopAfter=%d)", i, stopAfter)
				}
				if outcome.Value != nil {
					t.Fatalf("masked condition %d should have nil value", i)
				}
			} else {
				if outcome.Masked {
					t.Fatalf("condition %d should not be masked", i)
				}
				if outcome.Value == nil || *outcome.Value != vals[i] {
					t.Fatalf("condition %d: value mismatch", i)
				}
			}
		}

		expectedDecision := stopAfter >= 0
		if decision != expectedDecision {
			t.Fatalf("decision=%v, want %v", decision, expectedDecision)
		}
	})
}

// TestPropertyMcdcConditionCountPreserved verifies that evalMcdcChain always
// produces exactly one outcome per input condition.
func TestPropertyMcdcConditionCountPreserved(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		vals := genBoolSlice(t)
		op := rapid.SampledFrom([]string{"and", "or"}).Draw(t, "op")
		_, outcomes := evalMcdcChain(op, vals)

		if len(outcomes) != len(vals) {
			t.Fatalf("got %d outcomes, want %d", len(outcomes), len(vals))
		}
	})
}

// TestPropertyMcdcCapAt16 verifies that flattenConditionsAST rejects chains
// exceeding maxMcdcConditions = 16.
func TestPropertyMcdcCapAt16(t *testing.T) {
	rapid.Check(t, func(rt *rapid.T) {
		// Build chains of length 2-18 using only "a" params.
		n := rapid.IntRange(2, 18).Draw(rt, "n")
		params := make(map[string]bool)
		src := "a0 > 0"
		params["a0"] = true
		for i := 1; i < n; i++ {
			name := fmt.Sprintf("a%d", i)
			src += " && " + name + " > 0"
			params[name] = true
		}

		// Parse inline (can't use parseExprForTest with *rapid.T).
		fset := token.NewFileSet()
		fullSrc := "package p\nfunc _f() bool { return " + src + " }"
		f, err := parser.ParseFile(fset, "test.go", fullSrc, 0)
		if err != nil {
			rt.Fatalf("parse: %v", err)
		}
		fn := f.Decls[0].(*ast.FuncDecl)
		ret := fn.Body.List[0].(*ast.ReturnStmt)
		expr := ret.Results[0]

		result := flattenConditionsAST(expr, fset, params)

		if n > maxMcdcConditions {
			if result != nil {
				rt.Fatalf("expected nil for %d-condition chain (cap=%d)", n, maxMcdcConditions)
			}
		} else {
			if result == nil {
				rt.Fatalf("expected non-nil for %d-condition chain (cap=%d)", n, maxMcdcConditions)
			}
			if len(result.leaves) != n {
				rt.Fatalf("got %d leaves, want %d", len(result.leaves), n)
			}
		}
	})
}

// TestPropertyMcdcRoundtrip verifies that ConditionOutcome serializes and
// deserializes correctly (JSON roundtrip) via the executor-side struct.
func TestPropertyMcdcRoundtrip(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		masked := rapid.Bool().Draw(t, "masked")
		idx := rapid.IntRange(0, 15).Draw(t, "idx")
		val := rapid.Bool().Draw(t, "val")

		original := ConditionOutcome{
			ConditionIndex: idx,
			Masked:         masked,
			ConstraintJSON: `{"kind":"unknown"}`,
		}
		if !masked {
			b := val
			original.Value = &b
		}

		data, err := json.Marshal(original)
		if err != nil {
			t.Fatalf("marshal: %v", err)
		}

		var restored ConditionOutcome
		if err := json.Unmarshal(data, &restored); err != nil {
			t.Fatalf("unmarshal: %v", err)
		}

		if restored.ConditionIndex != original.ConditionIndex {
			t.Fatalf("condition_index: got %d, want %d", restored.ConditionIndex, original.ConditionIndex)
		}
		if restored.Masked != original.Masked {
			t.Fatalf("masked: got %v, want %v", restored.Masked, original.Masked)
		}
		if original.Value == nil && restored.Value != nil {
			t.Fatal("value should be nil for masked")
		}
		if original.Value != nil && (restored.Value == nil || *restored.Value != *original.Value) {
			t.Fatal("value mismatch after roundtrip")
		}
	})
}
