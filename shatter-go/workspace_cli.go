package main

import (
	"flag"
	"fmt"
	"io"
	"os"
	"sort"
	"time"

	"github.com/shatter-dev/shatter/shatter-go/workspace"
)

const (
	bytesPerKiB = 1024
	bytesPerMiB = bytesPerKiB * 1024
	bytesPerGiB = bytesPerMiB * 1024
)

// runWorkspaceSubcommand dispatches `shatter-go workspace <sub>`. It returns
// the process exit code.
func runWorkspaceSubcommand(args []string) int {
	if len(args) == 0 {
		fmt.Fprintln(os.Stderr, "workspace: missing subcommand (expected: gc)")
		return 2
	}
	switch args[0] {
	case "gc":
		return runWorkspaceGC(args[1:], os.Stdout, os.Stderr)
	default:
		fmt.Fprintf(os.Stderr, "workspace: unknown subcommand %q\n", args[0])
		return 2
	}
}

func runWorkspaceGC(args []string, stdout, stderr io.Writer) int {
	flags := flag.NewFlagSet("workspace gc", flag.ContinueOnError)
	flags.SetOutput(stderr)
	dryRun := flags.Bool("dry-run", false, "list candidates without deleting")
	keep := flags.Int("keep", workspace.DefaultGCKeepLastN, "keep the N most recent runs")
	maxAgeDays := flags.Int("max-age-days", int(workspace.DefaultGCMaxAge/(24*time.Hour)), "delete runs older than this many days")
	maxRunsBytes := flags.Int64("max-runs-bytes", workspace.DefaultGCMaxRunsBytes, "hard cap on total runs/ size in bytes")
	maxCacheBytes := flags.Int64("max-cache-bytes", workspace.DefaultGCMaxCacheBytes, "per-cache-dir size cap in bytes")
	if err := flags.Parse(args); err != nil {
		return 2
	}

	artifactWorkspace, err := workspace.Initialize(workspace.ResolveOptions{})
	if err != nil {
		fmt.Fprintf(stderr, "workspace gc: initialize workspace: %v\n", err)
		return 1
	}

	opts := workspace.GCOptions{
		KeepLastN:     *keep,
		MaxAge:        time.Duration(*maxAgeDays) * 24 * time.Hour,
		MaxRunsBytes:  *maxRunsBytes,
		MaxCacheBytes: *maxCacheBytes,
		DryRun:        *dryRun,
	}
	report, err := artifactWorkspace.RunGC(opts)
	if err != nil {
		fmt.Fprintf(stderr, "workspace gc: %v\n", err)
		return 1
	}

	printGCReport(stdout, report, *dryRun, artifactWorkspace.Root())
	return 0
}

func printGCReport(out io.Writer, report *workspace.GCReport, dryRun bool, root string) {
	prefix := "workspace gc"
	if dryRun {
		prefix = "workspace gc (dry-run)"
	}
	fmt.Fprintln(out, prefix)
	fmt.Fprintf(out, "  root: %s\n", root)
	fmt.Fprintf(out, "  runs/ scanned: %d\n", report.Scanned)

	if len(report.Candidates) == 0 {
		fmt.Fprintln(out, "  candidates: (none)")
	} else {
		fmt.Fprintln(out, "  candidates:")
		sorted := make([]workspace.GCCandidate, len(report.Candidates))
		copy(sorted, report.Candidates)
		sort.Slice(sorted, func(i, j int) bool {
			if sorted[i].Reason != sorted[j].Reason {
				return sorted[i].Reason < sorted[j].Reason
			}
			return sorted[i].Path < sorted[j].Path
		})
		for _, candidate := range sorted {
			identity := candidate.RunID
			if identity == "" {
				identity = candidate.Path
			}
			fmt.Fprintf(out, "    %-40s  %-10s  %s\n", identity, candidate.Reason, humanBytes(candidate.Size))
		}
	}

	fmt.Fprintf(out, "  runs/ size: %s -> %s\n",
		humanBytes(report.RunsSizeBefore), humanBytes(report.RunsSizeAfter))
	cacheNames := make([]string, 0, len(report.CacheSizes))
	for name := range report.CacheSizes {
		cacheNames = append(cacheNames, name)
	}
	sort.Strings(cacheNames)
	for _, name := range cacheNames {
		sizes := report.CacheSizes[name]
		fmt.Fprintf(out, "  %s size: %s -> %s\n", name, humanBytes(sizes.Before), humanBytes(sizes.After))
	}
	if dryRun {
		fmt.Fprintf(out, "  planned bytes: %s (not deleted)\n", humanBytes(report.BytesPlanned))
	} else {
		fmt.Fprintf(out, "  deleted: %d paths, %s\n", len(report.Deleted), humanBytes(report.BytesRemoved))
	}
}

func humanBytes(bytes int64) string {
	switch {
	case bytes >= bytesPerGiB:
		return fmt.Sprintf("%.2f GiB", float64(bytes)/float64(bytesPerGiB))
	case bytes >= bytesPerMiB:
		return fmt.Sprintf("%.2f MiB", float64(bytes)/float64(bytesPerMiB))
	case bytes >= bytesPerKiB:
		return fmt.Sprintf("%.2f KiB", float64(bytes)/float64(bytesPerKiB))
	default:
		return fmt.Sprintf("%d B", bytes)
	}
}
