# `explore --analyze-only` refused without a sandbox and shows no types; FunctionNotFound error unhelpful; false/incomplete help strings

- Priority: P3
- Type: bug
- Labels: cli,ux,analyze,error-handling
- Tracker action: new issue (str-qwua7.12 covers exit codes only; close .12 separately since all its cases already exit 2)
- Related: str-qwua7.12, str-qwua7.33, str-gg9v, str-qwua7.15
- Source findings: audit 2026-09-22 goals-18, cli-ux-19 (confirmed), analyze-only part of artifacts-16

<!-- body -->
## Items (reproduced at HEAD)
1. `shatter explore 05-unions.ts:computeArea --analyze-only` fails with `Error: refusing to execute target functions without a sandbox.` (exit 2), although analyze-only executes nothing. With `--allow-host-writes` it prints only `computeArea (05-unions.ts:17) params: 1, branches: 6`: no parameter names or types and no branch conditions (the walkthrough promises "types and conditions"). The output is plain text regardless of `--format`.
2. `explore arithmetic-v1.ts:doesNotExist` prints `Error: explore: all 1 attempted target(s) failed (build_failed=0, runtime_failed=0, timed_out=0)`, a breakdown of all zeros, and does not list the available functions.
3. `--dry-run` help (`shatter-cli/src/args.rs:676`) says "Requires --output", but `--dry-run` without `-o` works and exits 0.
4. The target help (`args.rs:501`) says `(.ts = TypeScript, .go = Go)` and omits `.rs`.
5. `SPEC.md:640-644` still says str-qwua7.12 exit-code work is pending, but all its cases now exit 2.

## Acceptance criteria
- `--analyze-only` bypasses the host-write refusal (host_writes gate aware of analyze-only). Its output lists parameters with types and branches with condition text, rendered in the active `--format`.
- A missing target function produces an `analyze_failed`/`not_found` category and a "did you mean" list of exported functions in that file.
- Help strings for `--dry-run` and the target argument match the behaviour.
- SPEC §2.11 note is updated. Close str-qwua7.12 with evidence.
- CLI tests for items 1-4.
