package instrument

import (
	"fmt"
	"os"
	"strconv"
	"testing"
	"time"

	"pgregory.net/rapid"
)

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
