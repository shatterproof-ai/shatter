package main

import "testing"

func TestParseArgsForwardsShatterFlags(t *testing.T) {
	options, forwarded, err := parseArgs([]string{"--version"})
	if err != nil {
		t.Fatal(err)
	}
	if options.repo != defaultRepo {
		t.Fatalf("repo = %q, want %q", options.repo, defaultRepo)
	}
	if len(forwarded) != 1 || forwarded[0] != "--version" {
		t.Fatalf("forwarded = %#v, want --version", forwarded)
	}
}

func TestParseArgsConsumesWrapperOptions(t *testing.T) {
	options, forwarded, err := parseArgs([]string{
		"--shatter-build",
		"continuous-test",
		"--shatter-repo=example/shatter",
		"--",
		"scan",
		"src",
	})
	if err != nil {
		t.Fatal(err)
	}
	if options.build != "continuous-test" {
		t.Fatalf("build = %q, want continuous-test", options.build)
	}
	if options.repo != "example/shatter" {
		t.Fatalf("repo = %q, want example/shatter", options.repo)
	}
	if len(forwarded) != 2 || forwarded[0] != "scan" || forwarded[1] != "src" {
		t.Fatalf("forwarded = %#v, want scan src", forwarded)
	}
}
