# Plan: Playwright Self-Contained Stack (kapow-7dz)

## Context

E2E tests currently depend on an externally running API and seeded database. The Playwright config auto-starts only the Vite dev server (`pnpm dev`) and assumes the API at `:8081` and a seeded PostgreSQL are already running. This makes E2E unreliable across environments.

The project already has an all-in-one Docker image (`make test-image-build`) that bundles PostgreSQL + fixture data + API + nginx into a single container. And `scripts/demo.sh` already demonstrates the pattern: build image → start container → wait for readiness → run Playwright. We'll adapt this pattern for E2E tests.

## Changes

### 1. Update `web/playwright.config.ts`

- Read `BASE_URL` from env (default `http://localhost:8080`)
- When `E2E_SELF_CONTAINED=1` is set, disable the `webServer` block (the Docker container serves everything)

```ts
const baseURL = process.env.BASE_URL || 'http://localhost:8080'
const selfContained = !!process.env.E2E_SELF_CONTAINED

// ...
use: { baseURL, ... },
webServer: selfContained ? undefined : { command: 'pnpm dev', url: baseURL, ... },
```

### 2. Create `scripts/e2e-self-contained.sh`

Based on the existing `scripts/demo.sh` pattern:

1. Build the all-in-one Docker image (`make test-image-build`)
2. Start container on a configurable port (default 8080, or `E2E_PORT`)
3. Wait for readiness (curl health check, 120s timeout)
4. Run Playwright with `BASE_URL` and `E2E_SELF_CONTAINED=1` set
5. Forward any args (e.g., `--grep @critical`)
6. Capture container logs to temp dir
7. Clean up container on exit (trap)
8. Exit with Playwright's exit code

### 3. Add Makefile targets

In root `Makefile`:
- `web-test-e2e-contained`: Run all E2E tests against self-contained stack
- `web-test-e2e-contained-critical`: Run only `@critical` tier

In `web/Makefile`:
- `e2e-test-contained`: delegates to the script

### 4. Update `make test-full`

Replace the `web-test-e2e` dependency with `web-test-e2e-contained` so full test runs are self-contained by default.

## Files to modify

| File | Change |
|---|---|
| `web/playwright.config.ts` | Add `BASE_URL` env + conditional `webServer` |
| `scripts/e2e-self-contained.sh` | New script (based on `demo.sh`) |
| `Makefile` | Add `web-test-e2e-contained` targets, update `test-full` |
| `web/Makefile` | Add `e2e-test-contained` target |

## Verification

1. `make web-test-e2e-contained-critical` — runs critical E2E tests against Docker container
2. Existing `make web-test-e2e` still works (unchanged for dev workflow)
3. Quality gate: `cd web && pnpm build && pnpm lint`
