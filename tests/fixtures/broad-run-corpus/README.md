# Broad-Run Validation Corpus

Stable synthetic fixture suite that captures Kapow-discovered failure
classes for `shatter scan`. The corpus is self-contained: it does not
require a live external Kapow checkout and its expected counters are
pinned in `manifest.yaml`.

## Failure classes covered

| Sub-fixture            | Failure class                                        |
|------------------------|------------------------------------------------------|
| `go-large-set/`        | Large source set; tiny denominator unless failed counted |
| `go-internal-pkg/`     | Go internal-package imports                          |
| `go-mixed-test-pkg/`   | `_test.go` siblings with mixed package names         |
| `ts-private-helpers/`  | TS private (non-exported) helpers                    |
| `ts-tsx-typeonly/`     | TSX + type-only syntax                               |
| `ts-browser-globals/`  | TS browser globals (`window`, `document`)            |
| `ts-missing-dep/`      | TS imports of packages not in `package.json`         |
| `mixed-rust-frontend/` | Rust frontend unavailable in mixed-language run      |
| `no-target-opaque/`    | No-target files: every function is fully opaque      |
| `stale-source-go/`     | Source files added/removed between runs              |

## Gate

Run the gate locally:

```sh
task broad-run-corpus
```

The gate (`tests/scripts/broad_run_validation.py`) invokes `shatter scan`
against each sub-fixture and asserts:

1. **Denominator integrity** — the sum of `completed_functions`,
   `failed_functions`, `skipped_functions_count`, and
   `unsupported_functions` equals `attempted_functions`, and
   `attempted_functions` is at least the manifest's `min_attempted`
   threshold (so silent skips can't shrink the denominator below
   what Kapow exposed).
2. **Artifact integrity** — every `file_path` referenced in `failed`,
   `skipped`, and `functions` arrays points to a real file on disk.
3. **Failure-class presence** — at least one `failed[].reason` matches
   the manifest's expected regex for each sub-fixture that declares one.
4. **Stale-source detection** — for `stale-source/`, scan, mutate the
   source set, rescan, and assert no dangling references to removed files.

The gate exits non-zero on any assertion failure and prints a
human-readable summary.

## Updating thresholds

When CLI behavior changes intentionally, edit `manifest.yaml` to
reflect the new counts and rerun the gate. Manifest fields are
documented at the top of the file.

## Adding a fixture

1. Create `tests/fixtures/broad-run-corpus/<name>/` with a minimal
   self-contained source tree.
2. Add an entry to `manifest.yaml` with `mode`, `language`, and
   either `min_attempted` (full_scan) or `stderr_pattern`
   (preflight_failure).
3. Run `task broad-run-corpus` and confirm the gate passes.
