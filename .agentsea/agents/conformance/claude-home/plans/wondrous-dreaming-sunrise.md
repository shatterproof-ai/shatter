# Demo Runner Scaffold (flt-wnb)

## Context

Flotsam needs a one-command demo runner that starts services, seeds data, and runs a Playwright walkthrough. This scaffold provides the orchestration and minimal boot-verification tests; future slices add feature-specific walkthrough steps. The existing `scripts/dev.sh`, `make api-seed`, and `make api-dev-token` already handle most pieces — the demo runner orchestrates them with deterministic reset and Playwright on top.

## Architecture

- **`demo/`** — standalone Node package at project root with `@playwright/test` as its only dependency (keeps ~200MB of browser binaries out of `web/node_modules`)
- **`scripts/demo.sh`** — shell orchestrator following `dev.sh` patterns (PID tracking, trap, color output)
- **Makefile targets** — `demo`, `demo-manual`, `demo-headless`, `demo-install`

## Files to Create

### `demo/package.json`
Minimal package with `@playwright/test` ^1.49.0. Scripts: `test` → `playwright test`, `report` → `playwright show-report`.

### `demo/tsconfig.json`
Standalone TS config. Target ES2022, module ESNext, moduleResolution bundler.

### `demo/playwright.config.ts`
Reads `DEMO_MODE` env var (default: `auto`). Mode matrix:

| Setting | auto | manual | headless |
|---------|------|--------|----------|
| headless | false | false | true |
| slowMo | 800 | 0 | 0 |
| trace | on | on | on-first-retry |

Common: `baseURL: http://localhost:8080`, chromium only, `workers: 1`, `retries: 0`, `timeout: 30000`. Reporter: HTML to `../logs/demo/report`, artifacts to `../logs/demo/artifacts`.

### `demo/tests/boot-verification.spec.ts`
Four tests matching real UI (verified from source):

1. **API health** — `GET http://localhost:8081/health` returns 200
2. **Home page loads** — navigate `/`, assert "Your second brain" visible
3. **Login with seed credentials** — fill Email "ketan@example.com", Password "testpassword123", click Login button, assert redirects to `/` and "Capture" button appears (auth state)
4. **Browse page shows seeded items** — after login, navigate `/browse`, assert a seeded item title visible

Manual mode: `page.pause()` calls gated by `if (process.env.DEMO_MODE === 'manual')`.

### `scripts/demo.sh`
Orchestration sequence:
1. Parse args: `MODE=$1` (default: auto), `--keep-data` flag
2. Create `logs/demo/` directory
3. Unless `--keep-data`: `docker compose -f compose/db.yml down -v` then `up -d --wait`
4. Load `.env`, set defaults (`DATABASE_URL`, `JWT_SECRET=dev-secret`, `ENV=development`)
5. Run migrations (`make -C api migrate-up`)
6. Seed data (`make -C api seed`)
7. Install demo deps if needed (`cd demo && pnpm install && pnpm exec playwright install chromium`)
8. Install web deps if needed
9. Start API server → `logs/demo/api.log`
10. Start Vite dev server → `logs/demo/web.log`
11. Poll health endpoints (8081/health + 8080) with 30s timeout
12. Run Playwright: `cd demo && DEMO_MODE=$MODE pnpm exec playwright test`
13. Print summary (pass/fail, log locations, report path)
14. Exit with Playwright's exit code; trap handles cleanup

## Files to Modify

### `Makefile`
Add to `.PHONY` list and append targets:
```makefile
demo:          bash scripts/demo.sh auto
demo-manual:   bash scripts/demo.sh manual
demo-headless: bash scripts/demo.sh headless
demo-install:  cd demo && pnpm install && pnpm exec playwright install --with-deps chromium
```

### `.gitignore`
Add `logs/` entry under existing Playwright section.

## Verification

1. `make demo-headless` from clean state → DB starts, seeds, servers start, Playwright runs 4 tests, exits 0
2. Stop API mid-run → `make demo-headless` exits non-zero
3. `logs/demo/` contains `api.log`, `web.log`, `runner.log`, `report/index.html`
4. `make demo` opens visible browser with auto-stepping
5. `make demo-manual` pauses at key points for interaction
6. Ctrl-C during any mode kills all background processes cleanly

## Quality Gate
This adds only scripts and a new `demo/` package (no Go or web source changes), so: verify scripts are executable, have correct syntax (`bash -n`), and `make demo-headless` runs successfully.
