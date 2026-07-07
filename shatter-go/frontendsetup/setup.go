// Package frontendsetup wires the hint-config-aware invocation planner into a
// protocol Handler so every frontend binary shares one registration: the
// embedded shatter-go frontend AND any custom frontend generated for projects
// that declare go_runtime_values / native generators (shatter-cli
// build_frontend). Before str-79t9 only the embedded main.go registered this
// planner, so configured .shatter `defaults`/`generators` were silently ignored
// for every target whenever a custom frontend was built.
package frontendsetup

import (
	"encoding/json"
	"strings"

	"github.com/shatter-dev/shatter/shatter-go/config"
	"github.com/shatter-dev/shatter/shatter-go/planner"
	"github.com/shatter-dev/shatter/shatter-go/protocol"
)

// RegisterDefaultPlanner installs the hint-config-aware invocation planner on h.
// Call it from every frontend main after constructing the Handler so
// get_invocation_plan resolves per-target hint config consistently.
func RegisterDefaultPlanner(h *protocol.Handler) {
	h.RegisterPlanner(func(
		requirements []protocol.InvocationRequirement,
		lookup func(string) *protocol.TargetContext,
	) ([]protocol.InvocationPlan, []protocol.UnsatisfiedRequirement) {
		opts := planner.PlanRequirementsOptions{
			PerTargetHints: hintConfigResolver(lookup),
		}
		return planner.PlanRequirements(requirements, lookup, opts)
	})
}

// hintConfigResolver builds a per-target PerTargetHints resolver backed by the
// .shatter/config.yaml loader. It looks up each target's source file via the
// planner-supplied analysis cache, walks upward for a config file, and
// translates the matched entry into the planner's hint surface.
//
// A zero-value PerTargetHints (empty file or no matched entry) preserves the
// pre-G3 planner behaviour (no overrides applied). Loader errors are ignored
// here because the safety-policy gate (str-hy9b.G4) is the canonical place a
// corrupt config surfaces to the operator.
func hintConfigResolver(lookup func(string) *protocol.TargetContext) func(string) planner.PerTargetHints {
	return func(targetID string) planner.PerTargetHints {
		ctx := lookup(targetID)
		if ctx == nil || ctx.Analysis == nil || ctx.Analysis.SourceFile == "" {
			return planner.PerTargetHints{}
		}
		analysis := ctx.Analysis
		file, err := config.Load(analysis.SourceFile)
		if err != nil {
			return planner.PerTargetHints{}
		}
		// str-rd0a: normalize SourceFile the same way the policy resolver does
		// (config.TargetRelpath). Without this, an absolute SourceFile never
		// matches filename-scoped `defaults`/`generators` globs, so per-function
		// hint config silently failed for scans even though `policy` worked.
		entry := file.MatchTarget(config.TargetRelpath(analysis.SourceFile), analysis.Name)
		hints := translateHintConfig(entry)
		if len(file.GoRuntimeValues) > 0 {
			hints.ConfiguredRuntimeValues = file.GoRuntimeValues
		}
		return hints
	}
}

// translateHintConfig converts a config.FunctionConfig into the planner's
// PerTargetHints (Defaults → ParamValueHint, Generators → type spelling,
// Mocks → Go expression). Empty maps yield a zero PerTargetHints.
func translateHintConfig(entry config.FunctionConfig) planner.PerTargetHints {
	hints := planner.PerTargetHints{}
	if len(entry.Defaults) > 0 {
		hints.Defaults = make(map[string]planner.ParamValueHint, len(entry.Defaults))
		for paramName, dv := range entry.Defaults {
			if len(dv.JSON) == 0 {
				continue
			}
			hints.Defaults[paramName] = planner.ParamValueHint{
				Literal:  json.RawMessage(dv.JSON),
				TypeHint: dv.TypeHint,
			}
		}
	}
	if len(entry.Generators) > 0 {
		hints.Generators = make(map[string]string, len(entry.Generators))
		for paramName, typeName := range entry.Generators {
			hints.Generators[paramName] = typeName
		}
	}
	if len(entry.Mocks) > 0 {
		hints.Mocks = make(map[string]string, len(entry.Mocks))
		for qualified, expression := range entry.Mocks {
			hints.Mocks[qualified] = expression
		}
	}
	if entry.Receiver != nil && strings.TrimSpace(entry.Receiver.Expression) != "" {
		receiver := *entry.Receiver
		hints.Receiver = &receiver
	}
	return hints
}
