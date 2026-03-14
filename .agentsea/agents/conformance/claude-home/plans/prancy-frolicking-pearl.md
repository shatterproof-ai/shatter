# str-28vd.3: Quality script entrypoints

## Context
Quality scripts already exist as untracked files in the main repo (`scripts/quality/`), and Makefile changes are staged but uncommitted. The worktree branch needs these files copied in, committed, validated, and pushed.

## Plan

1. **Copy existing files** from main repo into worktree:
   - `scripts/quality/lib/common.sh`
   - `scripts/quality/check-rust.sh`, `check-ts.sh`, `check-go.sh`, `check-docs.sh`, `check-meta.sh`, `check-tooling.sh`, `check-all.sh`, `pre-completion.sh`

2. **Update Makefile** in worktree with the quality targets (apply the same diff that exists on main)

3. **Validate syntax** — run `bash -n` on all scripts

4. **Commit** with prefix `str-28vd.3:`

5. **Push** branch

## Files
- `scripts/quality/lib/common.sh` — shared helpers
- `scripts/quality/check-{rust,ts,go,docs,meta,tooling,all}.sh` — per-domain checks
- `scripts/quality/pre-completion.sh` — agent pre-completion gate
- `Makefile` — add quality targets

## Verification
- `bash -n` passes on all scripts
- `make help` shows the new quality targets
