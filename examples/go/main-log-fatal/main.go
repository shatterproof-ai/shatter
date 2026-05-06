// Package main is the str-jeen.55 fixture for a CLI entrypoint whose body
// terminates the process via log.Fatal (which internally calls os.Exit(1)
// after writing the message). Same failure mode as main-os-exit: the
// launcher subprocess dies before writing a response, producing a
// misleading "launcher: subprocess exited unexpectedly" classification
// pre-fix. The analyzer-side filter on `func main()` prevents scan from
// dispatching it through the launcher in the first place.
package main

import "log"

func main() {
	log.Fatal("boom")
}

// Classify returns "even" when n is even and "odd" otherwise. Lives in the
// same CLI package as `main`; analyzer must still surface it after
// filtering main.
func Classify(n int) string {
	if n%2 == 0 {
		return "even"
	}
	return "odd"
}
