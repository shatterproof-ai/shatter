package main

import (
	"encoding/json"
	"fmt"
	"os"
	"os/signal"
	"syscall"

	"github.com/shatter-dev/shatter/shatter-go/config"
	"github.com/shatter-dev/shatter/shatter-go/launcher"
	"github.com/shatter-dev/shatter/shatter-go/planner"
	"github.com/shatter-dev/shatter/shatter-go/protocol"
	"github.com/shatter-dev/shatter/shatter-go/workspace"
)

func main() {
	// str-bni0: sweep transient launcher source dirs on signal-induced exit.
	// Go's default SIGTERM/SIGINT handling bypasses deferred cleanups, which
	// otherwise leaves `.shatter-launchers/` artefacts in the target project
	// tree when the parent CLI is interrupted.
	installLauncherSweepSignalHandler()

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
		lookup func(string) *protocol.TargetContext,
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

// installLauncherSweepSignalHandler arranges for transient launcher source
// directories registered by the launcher package to be removed before the
// process exits in response to SIGTERM or SIGINT (str-bni0). Without this
// hook, defers inside BuildLauncher are skipped on signal-induced exit and
// the target project tree is left with a stray `.shatter-launchers/` entry.
func installLauncherSweepSignalHandler() {
	ch := make(chan os.Signal, 1)
	signal.Notify(ch, syscall.SIGTERM, syscall.SIGINT)
	go func() {
		sig := <-ch
		launcher.SweepActive()
		// Re-raise the default disposition so the parent observes the same
		// exit signal it sent. Defaults: SIGTERM → exit 143, SIGINT → exit
		// 130; using os.Exit(128+signum) approximates that without
		// reinstalling the default handler.
		signum := 0
		if s, ok := sig.(syscall.Signal); ok {
			signum = int(s)
		}
		os.Exit(128 + signum)
	}()
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
		entry := file.MatchTarget(analysis.SourceFile, analysis.Name)
		hints := translateHintConfig(entry)
		if len(file.GoRuntimeValues) > 0 {
			hints.ConfiguredRuntimeValues = file.GoRuntimeValues
		}
		return hints
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
