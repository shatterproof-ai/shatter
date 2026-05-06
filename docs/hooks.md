# Local Git Hooks

Shatter uses local git hooks to run quality gates before commits and pushes.
Hooks delegate to Taskfile tasks so check logic lives in one place.

## Setup

```bash
./scripts/setup-hooks.sh
```

This is idempotent — run it any time. It appends a guarded section to your git
hooks without disturbing existing content (e.g. Beads integration).

`scripts/setup-dev.sh` calls this automatically during initial dev setup.

## What the hooks run

| Hook | Task | Scope |
|------|------|-------|
| `pre-commit` | `task core:clippy` | Rust tests + clippy |
| `pre-push` | `task check` | All language quality gates |

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
