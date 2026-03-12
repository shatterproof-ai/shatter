# Local Git Hooks

Shatter uses local git hooks to run quality gates before commits and pushes.
Hooks delegate to the repo-owned scripts in `scripts/quality/` so check logic
lives in one place.

## Setup

```bash
./scripts/setup-hooks.sh
```

This is idempotent — run it any time. It appends a guarded section to your git
hooks without disturbing existing content (e.g. Beads integration).

`scripts/setup-dev.sh` calls this automatically during initial dev setup.

## What the hooks run

| Hook | Script | Scope |
|------|--------|-------|
| `pre-commit` | `scripts/quality/check-rust.sh` | Rust tests + clippy |
| `pre-push` | `scripts/quality/check-all.sh` | All language quality gates |

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
