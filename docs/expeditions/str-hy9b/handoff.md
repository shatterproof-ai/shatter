# str-hy9b Expedition Handoff

- Expedition: `str-hy9b`
- Base branch: `str-hy9b`
- Base worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b`
- Status: `task_in_progress`
- Active task branch: `str-hy9b-26-j2-remove-legacy-direct-call-harness`
- Active task worktree: `/home/ketan/project/shatter/.claude/worktrees/str-hy9b-26-j2-remove-legacy-direct-call-harness`
- Last completed: `str-hy9b-25-j1-retirement-inventory (kept)`
- Next action: Continue `str-hy9b-26-j2-remove-legacy-direct-call-harness` by retiring the remaining `ExecuteFunction` legacy runtime, its `SHATTER_HARNESS_CACHE` / `SHATTER_HARNESS_SCRATCH` fallout, and its large test surface, then removing `wrapper/` from `build.Builder`. The launcher-backed runtime replacement is stable, the `shatter-harness` bridge resolves to the checked-in `shatter-go/harness` module, and both legacy `protocol` callsites plus the old `InstrumentFile*` API surface are gone.
- Primary branch: `main`
