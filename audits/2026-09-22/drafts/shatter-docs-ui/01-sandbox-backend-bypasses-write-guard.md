# SHATTER_SANDBOX_BACKEND disables the host-write guard for TS and Rust targets; docs recommend it anyway

- Priority: P1
- Type: bug
- Labels: sandbox,safety,docs,cli
- Tracker action: new issue (docs half was claimed by closed str-qwua7.8; code half is untracked)
- Related: str-qwua7.8 (closed, unfixed docs criterion), str-joyqu, str-gg9v
- Source findings: audit 2026-09-22 docs-01 (verdict confirmed)

<!-- body -->
## Problem
The documented remedy for the default-deny sandbox refusal is to set `SHATTER_SANDBOX_BACKEND` (for example `docker`). Only the Go frontend reads this variable and implements a backend. The CLI, however, treats *any* value of the variable as proof of confinement and skips its throwaway-directory `IsolationGuard` for every frontend. For TypeScript and Rust targets this removes all write protection: target code writes straight into the invoking directory.

README, QUICKSTART, SPEC and the refusal message all recommend this variable without a Go-only caveat. QUICKSTART recommends it for its TS example.

## Evidence (reproduced on main 9516036d)
- A TS target `touch()` calls `fs.writeFileSync('marker-*.txt')`.
- `SHATTER_SANDBOX_BACKEND=docker shatter explore w.ts:touch --max-iterations 5` exits 0 and leaves `marker-long.txt` and `marker-short.txt` in the cwd.
- The same run with `--allow-host-writes` (throwaway-dir guard) leaves the cwd clean.

## Current code facts
- `shatter-cli/src/host_writes.rs:142-145` returns `Ok(None)` (no IsolationGuard) whenever `SHATTER_SANDBOX_BACKEND` is set.
- The variable's only reader in rs/go/ts is `shatter-go/sandbox/runner.go:16`.
- Docs: `README.md:309-310` (comment says "OS sandbox (Go frontend)" but lists it as Recommended), `QUICKSTART.md:83-85`, `SPEC.md:604-606`, and `refusal_message()` in `host_writes.rs`.

## Acceptance criteria
- For a TS or Rust target, setting `SHATTER_SANDBOX_BACKEND` does not skip the IsolationGuard. A one-line warning says the backend is Go-only.
- There is a regression test per frontend (TS, Go, Rust): a target that writes a relative file under each opt-in (`--allow-host-writes`, `SHATTER_SANDBOX_BACKEND`) leaves the invoking cwd unchanged.
- README, QUICKSTART, SPEC §2.10 and the refusal message state that OS sandbox backends are Go-only and recommend `--allow-host-writes` for TS and Rust.
- SPEC §8 changelog row added.

## Suggested approach
Make the "sandboxed" decision per target language: a backend counts only when the target's frontend declares support for it. Consider adding a `sandbox_backends` capability to `protocol/parity-matrix.yaml` so the rule is data-driven.

## Scope
In: the host_writes decision, docs, the refusal text and per-frontend tests. Out: implementing OS sandboxes for TS or Rust, and relative-write redirection under `--allow-host-writes` (str-joyqu).
