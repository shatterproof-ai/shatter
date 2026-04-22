package main

import (
	"fmt"
	"os"

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
	if err := handler.Run(); err != nil {
		fmt.Fprintf(os.Stderr, "[shatter-go] Fatal: %v\n", err)
		os.Exit(1)
	}
}
