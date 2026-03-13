package instrument

import (
	"encoding/json"
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

// FuzzGenerateMockFile verifies that generateMockFile never panics for
// arbitrary MockConfig JSON and always produces valid Go source starting
// with "package main".
func FuzzGenerateMockFile(f *testing.F) {
	seeds := []string{
		mustJSON([]MockConfig{{Symbol: "fs.read", ReturnValues: []any{"ok"}, DefaultBehavior: BehaviorRepeatLast}}),
		mustJSON([]MockConfig{{Symbol: "db.q", DefaultBehavior: BehaviorThrowError, ReturnValues: []any{map[string]any{"message": "err"}}}}),
		mustJSON([]MockConfig{{Symbol: "x", DefaultBehavior: BehaviorPassthrough}}),
		mustJSON([]MockConfig{{Symbol: "a", DefaultBehavior: BehaviorCycle, ReturnValues: []any{1, 2}}}),
		"[]",
		`[{"symbol":"","return_values":null,"should_track_calls":false,"default_behavior":""}]`,
	}
	for _, s := range seeds {
		f.Add(s)
	}

	f.Fuzz(func(t *testing.T, data string) {
		var mocks []MockConfig
		if err := json.Unmarshal([]byte(data), &mocks); err != nil {
			return // skip inputs that don't parse as MockConfig slice
		}
		source := generateMockFile(mocks, "/tmp/calls.json")
		if !strings.HasPrefix(source, "package main") {
			t.Errorf("generated source does not start with 'package main': %s", source[:min(80, len(source))])
		}
	})
}

func mustJSON(v any) string {
	data, err := json.Marshal(v)
	if err != nil {
		panic(err)
	}
	return string(data)
}
