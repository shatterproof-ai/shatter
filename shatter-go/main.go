package main

import (
	"fmt"
	"os"

	"github.com/shatter-dev/shatter/shatter-go/protocol"
)

func main() {
	handler := protocol.NewHandler(os.Stdin, os.Stdout, os.Stderr)
	if err := handler.Run(); err != nil {
		fmt.Fprintf(os.Stderr, "[shatter-go] Fatal: %v\n", err)
		os.Exit(1)
	}
}
