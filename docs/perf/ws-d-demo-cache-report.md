# WS-D demo cache validation

Validation was run on 2026-08-24 from the `str-35vtk.6-warm-demo-cache`
worktree. The post-review production changes were tested at commit
`9e89526ba4a01cf342a1d0f248a81515360b71d4`. Each warm pair used a fresh,
dedicated `SHATTER_DEMO_CACHE`; the walkthrough and gauntlet pairs did not share
a root. All heavyweight commands used `scripts/gate-wrapper.sh`, and every
recorded gate wait was 0 seconds.

## Results

| Run | First | Second | Result |
|---|---:|---:|---|
| Warm walkthrough, whole `task` wall | 68.14s | 43.43s | 36.3% faster, but the first measurement includes 26s of Task dependency work |
| Warm walkthrough, governed script wall | 42s | 43s | No measurable improvement |
| Corrected warm gauntlet, whole wall | 289.36s | 245.65s | 15.1% faster, but load was not comparable |
| Corrected warm gauntlet, governed script wall | 289s | 246s | Both exit 0; gate wait 0s |
| Corrected warm gauntlet, aggregate `frontend.remote.execute.build` | 823.487ms | 1399.233ms | No build-phase improvement |
| Cold walkthrough | 43s, exit 0 | — | PASS |
| Corrected cold gauntlet | 227s, exit 0 | — | PASS |

The corrected warm gauntlets both completed all 60 steps, and the persistent
cache and harness directories survived both exits. The second whole run was
15.1% faster, but the recorded one-minute load averages were 30.59 and 9.62,
which fails the plan's 20% load-comparability condition. The predefined
build-phase fallback also did not improve. The result is therefore suggestive,
not controlled proof of a cache speedup; the raw numbers are retained to avoid
overstating the effect.

The corrected concurrency harness overrode an ambient sentinel cache with its
own `$RUN_ROOT/demo-cache` and ran two smoke tests and two warm gauntlets at
once. Both smoke tests exited 0 in 9s. The gauntlets exited 0 in 229s and 448s;
the second duration includes the intentional full-process lock wait. Both logs
emitted `waiting for shared demo cache lock`, and the harness verified that all
JSON and Markdown evidence parsed and contained no foreign run paths.

## Reproduction details

The corrected warm pair used:

```bash
SHATTER_DEMO_CACHE=/tmp/str-35vtk6-rerun-gauntlet.fVVnhe \
  /usr/bin/time -f %e -o /tmp/str-35vtk6-rerun-gauntlet-N.time \
  bash scripts/gate-wrapper.sh gauntlet-rerun-N \
  bash demo/gauntlet.sh --auto --delay 0 \
  --timing-dir /tmp/str-35vtk6-rerun-timingN.DIR
```

The concrete timing directories are
`/tmp/str-35vtk6-rerun-timing1.27mRJ9` and
`/tmp/str-35vtk6-rerun-timing2.S3pLgK`; the complete logs and wall-time files
use `/tmp/str-35vtk6-rerun-gauntlet-{1,2}.{log,time}`. The aggregate build phase
was derived separately for each directory with:

```bash
jq -s '[.[].phases[]
  | select(.phase_path | test("execute.build"))
  | .total_ms] | add // 0' TIMING_DIR/*.json
```

The corrected cold run was `task gauntlet-cold`, captured at
`/tmp/str-35vtk6-rerun-gauntlet-cold.log`. The isolated concurrency command was:

```bash
SHATTER_DEMO_CACHE=/tmp/this-value-must-be-overridden \
  bash scripts/gate-wrapper.sh demo-concurrency-rerun \
  bash scripts/concurrency-safety-check.sh .
```

Its durable-on-host run root is `/tmp/str-35vtk4-acceptance.anCxuU`; the harness
summary and per-run logs there record the shared run-local demo cache and
cross-contamination check.

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

Both blockers are now closed and landed on `main`: `str-gnagk` at
`067be28624cdd10730ebaf634ca742986cfa9153` and `str-6jwyw` at
`7e255d10cb5117676a6b2490294274c85e0ed2fc`. The landing guard is satisfied.

## Exact-candidate refresh

The rebased candidate `f27adf8eb2cf0b9f1f47a3157dabdeb439a63c6f`, which includes both
cache-invalidation fixes above, was validated again on 2026-08-25. Two warm
gauntlets sharing a fresh `SHATTER_DEMO_CACHE` completed all 60 steps in
641.89s and 470.34s, so the second run was 26.7% faster. The start load averages
were 137.06 and 55.87, however, so this remains evidence of a measurable warm
speedup rather than controlled proof under the plan's 20% load-comparability
rule. A cold gauntlet completed all 60 steps in 378.35s.

The exact-candidate concurrency harness also passed: its two warm gauntlets
exited 0 in 491s and 245s, its two smoke runs exited 0 in 9s each, and the
evidence parser found no foreign run paths. The worktree was clean after every
run. Exact-candidate `task --force meta`, `task smoke`, and `task e2e` also
passed.

Two warm walkthrough runs completed all 11 steps. Under one-minute host loads
above 40, their standalone TypeScript scans reported per-function execution or
task timeouts while the scripts continued successfully. The full E2E suite was
green, including the affected TypeScript exploration paths. The presentation
rubric remains MIXED for the pre-existing C/E/G/H reasons documented above.
An exact-candidate cold walkthrough subsequently completed all 11 steps in
45.27s at a 4.61 one-minute load average with no failed-function or timeout
summary.

The exact refresh logs and wall-time files use
`/tmp/str-35vtk6-exact-{gauntlet-run1,gauntlet-run2,gauntlet-cold,walkthrough-cold}.*`.
The concurrency harness retained its run root at
`/tmp/str-35vtk4-acceptance.C4SCUF` and its summary at
`/tmp/str-35vtk6-exact-concurrency.log`.
