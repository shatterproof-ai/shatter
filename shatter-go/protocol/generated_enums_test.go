package protocol

import (
	"slices"
	"sort"
	"testing"
)

// These tests reconcile the hand-written typed constants in this package
// against the generated slices in protocol_enums_gen.go (str-1hlk.8). The
// generated slices are derived directly from protocol/registry.yaml. If a
// future enum value is added in Go without a matching registry entry —
// or in registry.yaml without a matching Go constant — these tests fail.

func sortedCopy(in []string) []string {
	out := make([]string, len(in))
	copy(out, in)
	sort.Strings(out)
	return out
}

func equalStringSlices(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if a[i] != b[i] {
			return false
		}
	}
	return true
}

func TestErrorCodeConstantsMatchGenerated(t *testing.T) {
	handWritten := []string{
		ErrFileNotFound, ErrFunctionNotFound, ErrParseError,
		ErrInstrumentationFailed, ErrExecutionTimeout, ErrExecutionCrash,
		ErrVersionMismatch, ErrInvalidRequest, ErrCompilationError,
		ErrInternalError, ErrNotSupported, ErrPreflightFailed,
	}
	got := sortedCopy(handWritten)
	want := sortedCopy(AllErrorCodes)
	if !equalStringSlices(got, want) {
		t.Fatalf("hand-written Err* constants drifted from generated AllErrorCodes\nhand-written: %v\ngenerated:    %v", got, want)
	}
}

func TestSetupLevelConstantsMatchGenerated(t *testing.T) {
	got := make([]string, 0, len(ValidSetupLevels))
	for _, v := range ValidSetupLevels {
		got = append(got, string(v))
	}
	got = sortedCopy(got)
	want := sortedCopy(AllSetupLevels)
	if !equalStringSlices(got, want) {
		t.Fatalf("ValidSetupLevels drifted from generated AllSetupLevels\nhand-written: %v\ngenerated:    %v", got, want)
	}
}

func TestCommandCapabilitiesAreSubsetOfGenerated(t *testing.T) {
	registry := make(map[string]struct{}, len(AllCommands))
	for _, c := range AllCommands {
		registry[c] = struct{}{}
	}
	for _, cap := range CommandCapabilities {
		if _, ok := registry[cap]; !ok {
			t.Errorf("CommandCapabilities entry %q is not in generated AllCommands", cap)
		}
	}
}

func TestGeneratedSlicesAreSorted(t *testing.T) {
	for name, slice := range map[string][]string{
		"AllCommands":          AllCommands,
		"AllResponseStatuses":  AllResponseStatuses,
		"AllErrorCodes":        AllErrorCodes,
		"AllSetupLevels":       AllSetupLevels,
		"AllGeneratorKinds":    AllGeneratorKinds,
		"AllBranchTypes":       AllBranchTypes,
	} {
		if !sort.StringsAreSorted(slice) {
			t.Errorf("%s is not sorted: %v", name, slice)
		}
	}
}

func TestResponseStatusesIncludeError(t *testing.T) {
	if !slices.Contains(AllResponseStatuses, "error") {
		t.Fatalf("AllResponseStatuses must include the universal \"error\" status; got %v", AllResponseStatuses)
	}
}
