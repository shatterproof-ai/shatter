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
