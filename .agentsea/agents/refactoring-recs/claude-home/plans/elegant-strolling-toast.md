# str-5wl: explore --clean flag — Already Implemented

## Finding

The `--clean` flag for `explore` is **already fully implemented**. No code changes are needed.

### Evidence

| Component | Location | Status |
|-----------|----------|--------|
| CLI flag definition | `shatter-cli/src/main.rs:271-273` | Done |
| `run_explore()` parameter | `main.rs:888` | Done |
| Incremental plan gating (`!clean`) | `main.rs:1035` | Done |
| Wired from match arm | `main.rs:3056` | Done |
| CLI parsing test | `main.rs:4839-4853` | Done |
| Default-to-false test | `main.rs:4875-4888` | Done |
| Walkthrough stage 41 | `demo/walkthrough.sh:394-397` | Done |

### How it works

When `--clean` is set:
1. Line 1035: `!clean` is false, so the `incremental_plan` is set to `None`
2. `fresh_set` becomes empty (line 1057-1060)
3. All functions are treated as stale and explored from scratch
4. No existing spec is loaded or merged — fresh exploration output is written directly

## Plan

Close the issue as already done. Run verification tests to confirm everything passes, then `bd close str-5wl`.
