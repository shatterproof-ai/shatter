# Plan: Extension Demo Gate (flt-9jn)

## Context

The Chrome extension (`chrome-extension/`) and demo runner (`demo/`, `scripts/demo-api.sh`) are outside all CI and validation paths. Extension TypeScript errors or demo regressions won't be caught until someone manually runs them. This adds quality gates so these components are validated alongside the rest of the codebase.

## Changes

### 1. Add Makefile targets for extension (`Makefile`)

```makefile
ext-install:   cd chrome-extension && pnpm install
ext-build:     cd chrome-extension && pnpm run build
ext-typecheck: cd chrome-extension && pnpm run typecheck
```

Add `ext-typecheck` as a dependency of `test-standard` (fast, no DB needed).

Update `.PHONY` list accordingly.

### 2. Add extension detection to `scripts/ci/run-changed.sh`

Add a `chrome-extension/*` case alongside `api/*` and `web/*`. When extension files change, run `make ext-typecheck`.

### 3. Add extension step to CI standard job (`.github/workflows/ci.yml`)

In the `standard` job, before `make test-standard`:
- Install extension deps: `make ext-install`
- The `make test-standard` call will now include `ext-typecheck` (from step 1)

Also add `chrome-extension/pnpm-lock.yaml` to the pnpm cache key.

### 4. Add demo-api smoke test to full CI (`.github/workflows/full.yml`)

The `full.yml` workflow already has a PostgreSQL service container. After running `make test-full`, add steps to:
- Seed the database: `make api-seed`
- Start the API server in background
- Run `make demo-api` (headless curl-based GraphQL walkthrough)
- Kill the API server

This validates the full CRUD path without needing a browser.

## Files to modify

| File | Change |
|---|---|
| `Makefile` | Add `ext-install`, `ext-build`, `ext-typecheck`; add `ext-typecheck` to `test-standard` |
| `scripts/ci/run-changed.sh` | Add `chrome-extension/*` detection |
| `.github/workflows/ci.yml` | Add extension install + cache in `standard` and `changed` jobs |
| `.github/workflows/full.yml` | Add demo-api smoke test after test-full |

## Out of scope

- Browser-matrix testing for the extension
- Running Playwright demo tests in CI (requires full browser + running stack)
- Adding ESLint to the extension (no lint config exists yet)

## Verification

1. `make ext-typecheck` — must pass
2. `make ext-build` — must pass
3. `make test-standard` — must still pass (now includes ext-typecheck)
4. Review CI YAML with `act` or visual inspection for correctness
