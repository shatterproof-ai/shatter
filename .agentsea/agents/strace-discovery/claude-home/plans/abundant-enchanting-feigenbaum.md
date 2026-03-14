# Plan: CI Design Document (kapow-8ao.1)

## Context

Kapow has a single GitHub Actions workflow (`.github/workflows/ci.yml`) that runs `make test-standard` on push to main and PRs. The project needs a CI design document that captures the intended CI architecture — how jobs map to the existing test tiers, what should be required for branch protection, and how provider-specific workflow files stay thin by delegating to repo-local `make` targets and scripts.

## Approach

Create `docs/specs/ci-design.md` following the existing specs format (markdown with tables, H2/H3 sections, cross-references to related specs).

## Document Structure

1. **Overview** — Design philosophy: provider-agnostic CI via repo-local commands
2. **Test tier → CI job mapping** — How the three existing tiers (quick/standard/full) map to CI jobs
3. **Job definitions** — Each job with its scope, make targets, dependencies, runtime, and environment requirements
4. **Required status checks** — Which jobs must pass for branch protection
5. **Branch protection recommendations** — GitHub settings
6. **Delegation model** — How workflows stay thin (call make targets, no inline logic)
7. **Future considerations** — E2E, caching, deployment

## Key Files

- **Create**: `docs/specs/ci-design.md`
- **Reference**: `.github/workflows/ci.yml`, `Makefile`, `scripts/parallel-run.sh`

## Verification

- `make test-quick` passes (docs-only change, nothing should break)
- Document is consistent with existing Makefile targets and ci.yml
