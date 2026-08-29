# Local Git Hooks

Shatter uses local git hooks to run quality gates before commits and pushes.
Hooks delegate to Taskfile tasks so check logic lives in one place.

## Setup

```bash
./scripts/setup-hooks.sh
```

This is idempotent — run it any time. It installs or refreshes a versioned,
guarded section without disturbing existing content (e.g. Beads integration).

`scripts/setup-dev.sh` calls this automatically during initial dev setup.

## What the hooks run

| Hook | Task | Scope |
|------|------|-------|
| `pre-commit` | `scripts/precommit-rust.sh` | Tests + clippy for staged Rust crates |
| `pre-push` (feature branch) | `task affected` | Union of gates for the exact pushed head(s) |
| `pre-push` (main/master) | `task check` | Full landing quality gate |

Tag-only and deletion-only pushes run no product gate. Set
`SHATTER_FULL_PUSH=1` to force `task check` for any push.

## Skipping hooks

For a one-off bypass (e.g. WIP commit):

```bash
git commit --no-verify
git push --no-verify
```

## Checking status

```bash
./scripts/setup-hooks.sh --check
```

Reports whether the Shatter quality sections are installed without modifying
anything.
