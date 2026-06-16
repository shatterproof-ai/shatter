package main

import (
	"fmt"
	"os"
	"os/signal"
	"syscall"

	"github.com/shatter-dev/shatter/shatter-go/frontendsetup"
	"github.com/shatter-dev/shatter/shatter-go/launcher"
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
	// str-79t9: share the hint-config-aware planner wiring with every frontend
	// (including the custom frontend generated for go_runtime_values projects)
	// via frontendsetup, rather than registering it only here.
	frontendsetup.RegisterDefaultPlanner(handler)
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
