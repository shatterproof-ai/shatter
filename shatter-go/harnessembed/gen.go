//go:build ignore

// gen refreshes the embedded mirror from the harness module. Run via
// `go generate ./harnessembed` after editing shatter-go/harness/go.mod or
// runtime.go; harnessembed's drift test fails until you do.
package main

import (
	"fmt"
	"os"
	"path/filepath"
)

func main() {
	for src, dst := range map[string]string{
		filepath.Join("..", "harness", "go.mod"):     filepath.Join("mirror", "go.mod.txt"),
		filepath.Join("..", "harness", "runtime.go"): filepath.Join("mirror", "runtime.go.txt"),
	} {
		data, err := os.ReadFile(src)
		if err != nil {
			fmt.Fprintf(os.Stderr, "gen: %v\n", err)
			os.Exit(1)
		}
		if err := os.WriteFile(dst, data, 0o644); err != nil {
			fmt.Fprintf(os.Stderr, "gen: %v\n", err)
			os.Exit(1)
		}
	}
}
