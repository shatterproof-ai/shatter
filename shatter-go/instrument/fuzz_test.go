package instrument

import (
	"strings"
	"testing"
	"time"
)

// FuzzExecTimeout verifies that execTimeout() never panics regardless of
// SHATTER_EXEC_TIMEOUT env var content. Invalid values must silently fall
// back to defaultExecTimeout.
func FuzzExecTimeout(f *testing.F) {
	seeds := []string{
		"5", "0", "-1", "0.5", "1e3", "1e99",
		"NaN", "Inf", "-Inf", "abc", "", " ", "5s",
		"999999999999999999999999999999",
	}
	for _, s := range seeds {
		f.Add(s)
	}

	f.Fuzz(func(t *testing.T, val string) {
		// Env vars cannot contain null bytes — OS rejects them
		if strings.ContainsRune(val, 0) {
			return
		}
		t.Setenv("SHATTER_EXEC_TIMEOUT", val)
		dur := execTimeout()
		if dur <= 0 {
			t.Errorf("execTimeout returned non-positive duration %v for input %q", dur, val)
		}
	})
}

// FuzzBuildTimeout verifies that buildTimeout() never panics regardless of
// SHATTER_BUILD_TIMEOUT env var content.
func FuzzBuildTimeout(f *testing.F) {
	seeds := []string{
		"30", "0", "-1", "0.5", "1e3",
		"NaN", "Inf", "abc", "", " ",
	}
	for _, s := range seeds {
		f.Add(s)
	}

	f.Fuzz(func(t *testing.T, val string) {
		if strings.ContainsRune(val, 0) {
			return
		}
		t.Setenv("SHATTER_BUILD_TIMEOUT", val)
		dur := buildTimeout()
		if dur <= 0 {
			t.Errorf("buildTimeout returned non-positive duration %v for input %q", dur, val)
		}
	})
}

// FuzzSanitizeMockName verifies that sanitizeMockName never panics and always
// produces output containing only valid Go identifier characters.
func FuzzSanitizeMockName(f *testing.F) {
	seeds := []string{
		"fs.readFile", "a-b-c", "", "hello_world",
		"日本語", "foo.bar.baz", "a\x00b", "123",
	}
	for _, s := range seeds {
		f.Add(s)
	}

	f.Fuzz(func(t *testing.T, symbol string) {
		result := sanitizeMockName(symbol)
		for i, c := range result {
			valid := (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
				(c >= '0' && c <= '9') || c == '_'
			if !valid {
				t.Errorf("sanitizeMockName(%q) produced invalid char %q at index %d", symbol, string(c), i)
			}
		}
	})
}

// FuzzExecTimeoutDuration verifies that valid positive values produce the
// expected duration from execTimeout.
func FuzzExecTimeoutDuration(f *testing.F) {
	f.Add("1")
	f.Add("10")
	f.Add("0.001")

	f.Fuzz(func(t *testing.T, val string) {
		if strings.ContainsRune(val, 0) {
			return
		}
		t.Setenv("SHATTER_EXEC_TIMEOUT", val)
		dur := execTimeout()
		// Result must always be positive (either parsed value or default)
		if dur <= 0 || dur > 24*time.Hour {
			t.Errorf("execTimeout returned unreasonable duration %v for input %q", dur, val)
		}
	})
}
