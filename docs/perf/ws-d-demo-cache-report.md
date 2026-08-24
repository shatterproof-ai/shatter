# WS-D demo cache validation

Validation was run on 2026-08-24 from the `str-35vtk.6-warm-demo-cache`
worktree. Each warm pair used a fresh, dedicated `SHATTER_DEMO_CACHE`; the
walkthrough and gauntlet pairs did not share a root. All heavyweight commands
used `scripts/gate-wrapper.sh`, and every recorded gate wait was 0 seconds.

## Results

| Run | First | Second | Result |
|---|---:|---:|---|
| Warm walkthrough, whole `task` wall | 68.14s | 43.43s | 36.3% faster, but the first measurement includes 26s of Task dependency work |
| Warm walkthrough, governed script wall | 42s | 43s | No measurable improvement |
| Warm gauntlet, governed script wall | 211s | 217s | No measurable improvement |
| Warm gauntlet, aggregate `frontend.remote.execute.build` | 1044.187ms | 663.361ms | 36.5% faster; passes the predefined alternate 10% threshold |
| Cold walkthrough | 43s, exit 0 | — | PASS |
| Cold gauntlet | 226s, exit 0 | — | PASS |

The two warm gauntlets both completed all 60 steps. The second whole run was
not faster, so the build-phase result above is the performance evidence; the
whole-run numbers are retained to avoid overstating the effect. The recorded
one-minute load averages were 3.24 and 4.82, so the primary whole-run comparison
also fails the plan's 20% load-comparability condition.

The fresh-root concurrency harness ran two smoke tests and two warm gauntlets
at once. Both smoke tests exited 0 in 7s. The gauntlets exited 0 in 212s and
418s; the second duration includes the intentional full-process lock wait.
Both logs emitted `waiting for shared demo cache lock`, and the harness verified
that all JSON and Markdown evidence parsed and contained no foreign run paths.

`task smoke`, `task e2e`, `task --force meta`, the focused cache-mode regression,
shell syntax checks, ShellCheck, and diff checks passed. A first-run `shatter
init` generated the `.shatter/cache/` ignore entry; that entry is now tracked,
and subsequent runs leave the repository clean.

## Walkthrough review

The warm walkthrough completed all 11 steps with no unexpected error summary,
frontend protocol dump, stack trace, or crash. Function headers include source
locations and path counts, error outcomes include types and fixed messages, and
multi-function exploration has scannable per-function batch lines plus branch
coverage/completeness signals.

Against the full `walkthrough-review` rubric, the output remains **MIXED** for
pre-existing presentation reasons outside WS-D's cache lifecycle scope:

- Path tables show concrete calls but not abstract parameter-named path
  conditions, so criterion C fails.
- Each discovered path has one concrete example rather than 2–3, so criterion
  E fails.
- Default output includes iteration counts, elapsed times, `[info]`/`[progress]`
  prefixes, and later machine-readable export content, so criteria G and H fail.

Criteria A, B, D, F, I, J, and K pass. WS-D does not alter Shatter's report or
logging format; resolving the existing presentation gaps belongs to separate
CLI-output work.

## Landing guard

The tracker note on `str-35vtk.6` explicitly forbids enabling or landing warm
defaults until both cache-invalidation blockers are closed:

- `str-gnagk`: analysis cache is not invalidated by frontend changes.
- `str-6jwyw`: Rust harness cache is not invalidated by runtime-source changes.

This branch must remain unlanded while either blocker is open.
