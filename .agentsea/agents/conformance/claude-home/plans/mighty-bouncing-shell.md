# str-99w9: README CLI Reference Parity

## Context

The README CLI reference has drifted from the actual CLI surface. Three commands are entirely missing (`build-frontend`, `stale`, `test`), and existing command sections have significant flag drift — `explore` and `scan` have gained many new options (genetic algorithm, solver timeout, seeds, setup/teardown, core sampling, loop buckets, etc.) that aren't documented. The `scan` command's argument type changed from `<TARGETS>...` to `<DIRECTORY>`, and several flag names changed (`--timeout` → `--timeout-total`, `--report` → `--format`, `--output-dir` → `-o, --output`).

## Approach

Update `README.md` CLI Reference section to match the actual CLI. Rather than listing every flag (which creates maintenance burden), document the **most useful flags** per command and add a `--help` reference for full coverage.

### Changes

**File: `README.md` (lines 55–211)**

1. **Global options** (line 59): Add `--project-dir <DIR>` and `--color <WHEN>` to the global options line.

2. **`explore`** (lines 63–94): Add key missing flags to the table:
   - `--concolic` — use Z3-backed concolic explorer
   - `--genetic` — enable genetic algorithm explorer
   - `-o, --output PATH` — write spec JSON to file
   - `--solver-timeout SECS` — Z3 per-query timeout
   - `--memory-limit MB` — frontend process memory limit
   - `--timeout-explore SECS` — per-function wall-clock timeout
   - `--clean` — force full re-exploration
   - `--dry-run` — print stale/fresh/removed without exploring
   - `--seeds-dir DIR` / `--no-seeds` — cross-function seed pool
   - `--setup-timeout SECS` / `--fail-on-setup-error` — setup/teardown control
   - `--loop-buckets` — loop iteration bucketing
   - Add note: "Run `shatter explore --help` for the complete option list."

3. **`scan`** (lines 96–126): Fix argument (`<DIRECTORY>` not `<TARGETS>...`), fix renamed flags, add key missing flags:
   - Fix: argument is `<DIRECTORY>`
   - Fix: `--timeout` → `--timeout-total`, default 300
   - Fix: `--report` → `--format`, `--output-dir` → `-o, --output`
   - Remove: `--emit-tests-dir` (not in current CLI)
   - Add: `--language`, `--include`/`--exclude`, `--changed`/`--since`, `--all`
   - Add: `--core-sample`/`--seed`/`--batch` — progressive sampling
   - Add: `--stratum` — call graph layer filter
   - Add: `--dry-run`, `--resume`, `--mock-config`
   - Add: `--genetic`, `--solver-timeout`, `--memory-limit`, `--seeds-dir`, `--setup-timeout`
   - Add `--help` reference note

4. **`export-tests`** (lines 128–151): Add `--memory-limit`. Add `--help` reference.

5. **`run`** (lines 153–174): Add `--solver-timeout`, `--memory-limit`. Add `--help` reference.

6. **`diff`** (lines 176–193): Fine as-is. Add `--help` reference.

7. **`spec-diff`** (lines 195–211): Fine as-is. Add `--help` reference.

8. **Add `build-frontend`** section (new):
   - `<LANGUAGE>` — "go" or "rust"
   - `--config PATH` — path to `.shatter/` directory
   - `-o, --output DIR` — output directory (default: `.shatter/bin/`)

9. **Add `stale`** section (new):
   - `<SOURCE>` — source file to analyze
   - `<SPEC>` — spec JSON file to compare against
   - `--format` — text or json
   - `--cache-dir` / `--no-cache` — cross-file dependency tracking
   - Exit code semantics

10. **Add `revalidate`** section (new — not currently in README):
    - `<SOURCE>` — source file whose cached behaviors to revalidate
    - `--cache-dir`, `--format`
    - Exit code semantics

11. **Add `test`** section (new):
    - `--all` — bypass impact analysis
    - `--record` — refresh coverage map
    - `--tier` — run specific test tier
    - `--base REF` — git ref for change detection
    - `--dry-run`, `--prioritize`, `--budget`
    - Exit code semantics

### Style decisions
- Keep existing table format for flags
- Focus tables on the most-used flags (roughly 8-15 per command)
- Every command section ends with: "Run `shatter <cmd> --help` for the complete option list."
- Order new commands after spec-diff (matching `--help` command order)

## Verification

1. Visual: confirm every command from `shatter --help` has a README section
2. Spot-check: verify flag names/defaults in README match `--help` output
3. No code changes, so no tests needed
