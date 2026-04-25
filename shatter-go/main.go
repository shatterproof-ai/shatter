package main

import (
	"encoding/json"
	"fmt"
	"os"

	"github.com/shatter-dev/shatter/shatter-go/config"
	"github.com/shatter-dev/shatter/shatter-go/planner"
	"github.com/shatter-dev/shatter/shatter-go/protocol"
	"github.com/shatter-dev/shatter/shatter-go/workspace"
)

func main() {
	if len(os.Args) >= 2 && os.Args[1] == "workspace" {
		os.Exit(runWorkspaceSubcommand(os.Args[2:]))
	}

	artifactWorkspace, err := workspace.Initialize(workspace.ResolveOptions{})
	if err != nil {
		fmt.Fprintf(os.Stderr, "[shatter-go] Fatal: initialize workspace: %v\n", err)
		os.Exit(1)
	}

	handler := protocol.NewHandlerWithWorkspace(os.Stdin, os.Stdout, os.Stderr, artifactWorkspace)
	handler.RegisterPlanner(func(
		requirements []protocol.InvocationRequirement,
		lookup func(string) *protocol.FunctionAnalysis,
	) ([]protocol.InvocationPlan, []protocol.UnsatisfiedRequirement) {
		opts := planner.PlanRequirementsOptions{
			PerTargetHints: hintConfigResolver(lookup),
		}
		return planner.PlanRequirements(requirements, lookup, opts)
	})
	if err := handler.Run(); err != nil {
		fmt.Fprintf(os.Stderr, "[shatter-go] Fatal: %v\n", err)
		os.Exit(1)
	}
}

// hintConfigResolver builds a per-target PerTargetHints resolver backed by
// the .shatter/config.yaml loader. It looks up each target's source file
// via the planner-supplied analysis cache, walks upward for a config file,
// and translates the matched entry into the planner's hint surface.
//
// Returning a zero-value PerTargetHints when the file or matched entry is
// empty preserves the pre-G3 planner behaviour (no overrides applied).
// Loader errors are ignored at this layer because the existing safety-policy
// gate (str-hy9b.G4) is the canonical place where a corrupt config surfaces
// to the operator — silently re-failing here would double-report.
func hintConfigResolver(lookup func(string) *protocol.FunctionAnalysis) func(string) planner.PerTargetHints {
	return func(targetID string) planner.PerTargetHints {
		analysis := lookup(targetID)
		if analysis == nil || analysis.SourceFile == "" {
			return planner.PerTargetHints{}
		}
		file, err := config.Load(analysis.SourceFile)
		if err != nil {
			return planner.PerTargetHints{}
		}
		entry := file.MatchTarget(analysis.SourceFile, analysis.Name)
		return translateHintConfig(entry)
	}
}

// translateHintConfig converts a config.FunctionConfig into the planner's
// PerTargetHints. The translation is:
//
//   - Defaults map → ParamValueHint (param name → JSON literal + type hint)
//   - Generators map → param name → registered runtime-value type spelling
//   - Mocks map → qualified function name → Go expression
//
// Empty maps and nil sub-fields produce a zero PerTargetHints so the
// planner behaves identically to the pre-G3 path when the config has no
// matching entry.
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
	return hints
}
