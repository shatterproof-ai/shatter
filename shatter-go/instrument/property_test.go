package instrument

import (
	"fmt"
	"os"
	"regexp"
	"strconv"
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

func TestPropertyBuildTimeoutAlwaysPositive(t *testing.T) {
	rapid.Check(t, func(t *rapid.T) {
		val := rapid.OneOf(
			rapid.StringMatching(`[0-9]{1,5}(\.[0-9]{1,3})?`),
			rapid.StringMatching(`[a-zA-Z ]{0,10}`),
			rapid.Just(""),
			rapid.Just("0"),
			rapid.Just("-1"),
		).Draw(t, "envVal")

		os.Setenv("SHATTER_BUILD_TIMEOUT", val)
		defer os.Unsetenv("SHATTER_BUILD_TIMEOUT")
		dur := buildTimeout()

		if dur <= 0 {
			t.Fatalf("buildTimeout() returned non-positive %v for input %q", dur, val)
		}
	})
}

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

		os.Setenv("SHATTER_BUILD_TIMEOUT", val)
		defer os.Unsetenv("SHATTER_BUILD_TIMEOUT")
		dur = buildTimeout()
		if dur > maxDur {
			t.Fatalf("buildTimeout()=%v exceeds max %v for input %q", dur, maxDur, val)
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
		source := generateMockFile(mocks, "/tmp/calls.json")
		if !contains(source, "panic(msg)") {
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
		source := generateMockFile(mocks, "/tmp/calls.json")
		if !contains(source, "package main") {
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
		source := generateMockFile(mocks, "/tmp/calls.json")
		safeName := sanitizeMockName(symbol)
		if contains(source, "ShatterMock_"+safeName) {
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
		source := generateMockFile(mocks, "/tmp/calls.json")
		safeName := sanitizeMockName(symbol)

		if !contains(source, "func ShatterMock_"+safeName+"(args ...any) any") {
			t.Fatalf("missing panic variant for %q", symbol)
		}
		if !contains(source, "func ShatterMockErr_"+safeName+"(args ...any) (any, error)") {
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
		source := generateMockFile(mocks, "/tmp/calls.json")
		hasTracking := contains(source, fmt.Sprintf(`shatterRecordMockCall(%q`, symbol))

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
