# Plan: Critical E2E CI Design Document

## Context

E2E tests exist (11 spec files, ~15 @critical tests) with Playwright config that's CI-aware (retries, single worker), but E2E is **not integrated into GitHub Actions** — CI only runs `test-standard` (no DB, no services). This task creates a design document defining how the critical E2E lane should work in CI.

## Deliverable

Single file: `docs/specs/e2e-ci-design.md`

## Content outline

Based on exploration of existing setup:

1. **Overview** — What the critical E2E lane is and why it exists
2. **Test inventory** — ~15 @critical tests across 5+ spec files, covering core user journeys (search, filters, pagination, auth, navigation)
3. **Deterministic prerequisites**:
   - All-in-one Docker image (already exists: `make test-image-build/run`)
   - Seeded fixture data (`common/src/main/resources/data/`)
   - Playwright + Chromium installed
   - No external service dependencies
4. **Scope boundaries** — Critical vs standard vs comprehensive tiers
5. **Runtime expectations** — Target <2min, 0% flakiness tolerance (2 retries in CI already configured)
6. **CI integration** — New GitHub Actions job using Docker test image, `make web-test-e2e-critical`
7. **Branch protection** — Critical E2E as required check
8. **Relationship to broader E2E** — Full suite as release-only gate

## Key facts from exploration

- Playwright config: `retries: CI ? 2 : 0`, `workers: CI ? 1 : undefined`, `forbidOnly: CI`
- Makefile: `web-test-e2e-critical` → `pnpm test:e2e --grep @critical`
- Docker test image: `make test-image-build` produces self-contained image with PG + API + frontend
- Current CI: `.github/workflows/ci.yml` has single `test-standard` job

## Verification

- `make test-quick` passes (docs-only, no code changes)

## Style

Follow existing `docs/specs/` format: title, intro paragraph, related specs links, `---` separators, tables for structured data.
