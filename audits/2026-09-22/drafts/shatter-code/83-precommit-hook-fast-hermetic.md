# Pre-commit hook runs the full shatter-core/shatter-cli test suite (incl. E2E) on every commit: ~60 s median, fragile, main driver of --no-verify

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | task |
| priority | P1 |
| labels | git-hooks,quality-gates,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-35vtk.24, str-35vtk.25, str-npdt, str-dl2pj |
| source findings | sessions-04 |

<!-- body -->
## Problem

Every commit runs `cargo test` for the whole shatter-core/cli packages plus clippy, and pre-push runs `task affected`, then the land verifier and pre-push `task check` on main rerun the gates: each change is gated three to four times. Hook failures unrelated to the diff (unbuilt TS dist in fresh worktrees, ambient /tmp config, Go build timeouts under load) preceded most --no-verify bypasses observed in sessions.

## Current code facts / evidence

- `scripts/precommit-rust.sh:30-36` runs `cargo test -p shatter-core/-p shatter-cli` (full suite incl. integration tests) then clippy; also shatter-rust and shatter-rust-runtime tests when changed.
- Hooks (`.git/hooks/pre-commit`, `pre-push`) do not use run-heavy.
- Session measurements since 09-04: commit with hooks median 60 s / p90 124 s vs 2 s with --no-verify; push median 80 s foreground / 308 s background, max 728 s.
- Observed hook failures: 'TypeScript frontend not built: .../shatter-ts/dist/main.js does not exist' (fresh worktree); discover_configs tests failing on ambient /tmp/.shatter (str-dl2pj); Go build timeout at load 100-170.

## Acceptance criteria

- pre-commit runs only fast, hermetic checks on staged crates (fmt/check/clippy -D warnings), target ≤30 s, no tests requiring built frontends.
- pre-push accepts a fresh verifier/affected receipt for the same tree (str-35vtk.24/.25) instead of re-running.
- Hook-invoked gates go through run-heavy.
- Per-hook budget documented in CONTRIBUTING.md and AGENTS.md.

## Suggested approach

Implementer's choice within the acceptance criteria above.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: sessions-04 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-35vtk.24, str-35vtk.25, str-npdt, str-dl2pj
