package main

// Fixture for str-jeen.55: synthetic CLI entrypoint `func main()` must be
// excluded from analyzer output so it never becomes an executable target
// (executing main as a target produces "launcher: subprocess exited
// unexpectedly" failures when the body calls os.Exit / log.Fatal). Non-main
// helpers in the same package must remain discoverable.

import "fmt"

func main() {
	fmt.Println(Helper(7))
}

// Helper is a non-main free function the analyzer must still surface.
func Helper(n int) int {
	return n + 1
}
