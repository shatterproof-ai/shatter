# CI Workflow Gate — Implementation Plan

## Context

The repo has well-structured Make-based quality tiers (`test-quick`, `test-standard`, `test-full`) but no CI workflow enforcing them. Quality depends on developers remembering to run checks locally. This plan adds a GitHub Actions workflow that runs `make test-standard` on every push/PR, blocking merges on failure.

## Files to Create/Modify

1. **Create** `.github/workflows/ci.yml` — GitHub Actions workflow
2. **Edit** `README.md` — Add CI section documenting the workflow

## Workflow Design: `.github/workflows/ci.yml`

**Trigger**: `push` to `main` + all `pull_request` events

**Environment**: Ubuntu latest with Go 1.23, Node 22, pnpm 9

**Steps**:
1. Checkout code
2. Set up Go 1.23 with module caching
3. Set up Node 22 + pnpm 9 with pnpm store caching
4. `pnpm install` (web deps)
5. `make test-standard` — runs the full standard gate:
   - Go unit tests (`-race -short`)
   - TypeScript build (`pnpm build`)
   - ESLint (`--max-warnings 0`)
   - Web unit tests (Vitest)
   - Go lint (`go vet` + `golangci-lint`)

No database needed — `test-standard` is specifically designed to run without external services (~30-60s).

**golangci-lint**: Install via `golangci/golangci-lint-action` (standard approach for CI).

## README Update

Add a "## CI" section after "## Local Development Setup" (line ~200, before "## Project Structure") with:
- Badge showing workflow status
- Brief description of what the gate runs
- Reference to `make test-standard`

## Verification

1. Validate YAML syntax with `python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"`
2. Confirm README renders correctly
3. Push branch and verify workflow appears in GitHub Actions
