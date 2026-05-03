# Broad-run validation corpus (str-jeen.14)

Stable in-tree corpus that exercises Kapow-discovered broad-run failure
classes without requiring a live Kapow checkout. Pairs with a documented
local gate that asserts denominator, artifact integrity, stale-source
detection, and failure-class counts against per-fixture thresholds.

The gate is intentionally **not** wired into mandatory CI (per the
str-jeen.14 task description). Run it locally before any change touching
broad-run behavior — `shatter scan` orchestration, no-target classifiers,
preflight, frontend availability handling, or report artifact emission.

## Layout

```
tests/broad-run-corpus/
  manifest.yaml             # declarative fixture + threshold registry
  ts/
    private-helpers/        # str-jeen.9 — private helper not exported
    tsx-type-only/          # str-jeen.29, .22 — TSX + .d.ts siblings
    project-aliases/        # str-jeen.27, .28 — tsconfig paths
    missing-dep-preflight/  # str-jeen.26 — preflight on missing dep
    browser-globals/        # str-jeen.30 — host globals
  rust-unavailable/         # str-jeen.13 — frontend missing on PATH
  no-target-categories/     # str-jeen.21–.25 — one file per NoTargetReason
  source-churn/
    initial/                # phase 1 source set
    added-file/             # files added between phase 1 and phase 2
  dangling-artifacts/       # str-jeen.4 — artifact path resolution

scripts/
  broad_run_validation_gate.py        # gate driver
  test_broad_run_validation_gate.py   # unit + property tests
```

Existing Go fixtures under `examples/go/` already cover the Go failure
classes (str-jeen.32–.35). The manifest references them rather than
duplicating fixture authorship.

## Running

```bash
# Build the CLI first if needed:
cargo build -p shatter-cli

# Full corpus, verbose:
npx task broad-run-validation
# or directly:
python3 scripts/broad_run_validation_gate.py -v

# A single fixture:
python3 scripts/broad_run_validation_gate.py --filter ts-tsx-type-only -v

# Just list fixture IDs:
python3 scripts/broad_run_validation_gate.py --list

# Unit/property tests for the gate's pure functions:
npx task broad-run-validation-tests
```

The gate exits 0 on PASS, 1 on regression, 2 on bad invocation. Per-fixture
PASS/FAIL summary plus failure messages print to stdout.

## Manifest schema

See the comment block at the top of `tests/broad-run-corpus/manifest.yaml`.
The key fields:

- `command`: `scan` (default), `scan_with_path_stripped`, or `source_churn`.
  - `scan_with_path_stripped` invokes the gate with `PATH=""` and a tempdir
    cwd, matching the technique in
    `shatter-cli/tests/rust_frontend_availability_test.rs`. Used by the
    Rust-unavailable fixture so neither `find_on_path("shatter-rust")` nor
    `./shatter-rust/target/...` resolves.
  - `source_churn` is a two-phase orchestration in the gate: copy
    `initial/` to a tempdir, scan, copy `added-file/*` over, scan again,
    compare. **No sleep-based coordination** — sequencing is in the gate.
- `expected.run_must_succeed`: the scan must exit 0 (default true).
- `expected.{min,max}_*_functions`: bounds on the codebase rollup ints.
- `expected.no_target_reasons`: list of `(file, reason)` pairs the report
  must contain. Use `reason_one_of:` when the wire format is in flux
  across str-jeen.21–.25.
- `expected.artifact_paths_must_resolve` (default true): every absolute
  path in the JSON report under the worktree or the canonical artifact
  trees (`/.shatter`, `/shatter-artifacts`) must exist on disk. Catches
  the dangling-artifact regression class.
- `tighten_when`: human-readable note tying loose thresholds to the bead
  that should drive them tighter.

## Current thresholds reflect open P1s

The parent epic `str-jeen` still has open P1 bugs whose fixes will tighten
the thresholds in this manifest:

| Field / fixture | Bead | Tighten when |
| --- | --- | --- |
| `go-internal-method.min_completed_functions` | str-jeen.32 | Internal-package launcher synthesis lands so internal-package functions complete instead of failing at launcher build. |
| `ts-private-helpers.max_total_discovered_functions` | str-jeen.9 | Private-helper exclusion is firm; drop max from 2 to 1. |
| `no-target-categories.no_target_reasons` | str-jeen.21–.25 | Sharpen reason assertions as classifiers land per-language. |
| `source-churn.churn` | str-jeen.3 | Switch to manifest source-snapshot diff once landed; phase2 must record the file-set delta in the run manifest itself, not just rediscover targets. |
| Whole-source denominator across all fixtures | str-jeen.17, .20 | Once whole-source denominator is wired, add `expected.denominator_*` rows tying the corpus's known source-line totals to reported denominators. |
| Failure-class line weights | str-jeen.6 | Add `expected.failure_line_weight_*` rows once the line-weight failure-impact reporter lands. |

When you fix a bug above, the protocol is:

1. Run `npx task broad-run-validation` from a fresh worktree to confirm
   the gate now reports the *new* (tighter) numbers.
2. Update `manifest.yaml`'s threshold for the affected fixture(s) and
   strike the corresponding row from the table above.
3. Rerun the gate to confirm PASS at the tightened threshold.

The intent is that `manifest.yaml` is a regression ratchet — every bead
that lands sharpens it, and any future regression that loosens behavior
trips the gate immediately.

## Out of scope (and why)

- **Mandatory CI wiring.** Excluded by the str-jeen.14 task description;
  add a CI workflow only after coordinating on host-image cost and
  flake-tolerance for end-to-end TS/Go scans.
- **Tightening thresholds against open P1s.** The bug-fix beads
  (str-jeen.17, .19, .20, .42, .43, .44) own those tightenings; this
  task only makes the regressions detectable.
- **Source-set classifier coverage** (str-jeen.37–.39). The gate inspects
  the whole-codebase classifier output once it exists; until then this
  doc records the gap and `tighten_when:` rows track the dependency.

## See also

- `examples/go/{internal-method,mixed-package,multi-import-wrapper,rewrite-syntax}/README.md` — Go-side fixtures the corpus reuses.
- `shatter-cli/tests/rust_frontend_availability_test.rs` — Rust-unavailable
  test that the corpus's `rust-unavailable/` fixture mirrors.
- `shatter-core/src/protocol.rs` — `NoTargetReason` enum the
  `no-target-categories/` fixture is keyed against.
- `docs/validation/2026-04-go-frontend-kapow-rerun.md` — original
  Kapow-rerun report whose failure classes this corpus codifies.
