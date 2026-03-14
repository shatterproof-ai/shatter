# Plan: flt-it8.32.18 — Vendor-specific CI impl (GitHub Actions)

## Context

The repo has a complete set of vendor-neutral CI scripts in `scripts/ci/` with
two control flags (`FLOTSAM_STRICT=1`, `FLOTSAM_RUN_QUALITY_GATES=1`). The
blueprint at `docs/ci/blueprint.md` specifies exactly how to wire these into a
CI platform. This task translates them into GitHub Actions workflows, enables
strict mode, and publishes coverage/analyzer artifacts.

---

## Deliverables

### 1. `.github/workflows/ci.yml` — PR / push workflow

**Triggers:**
- `pull_request` targeting `main`
- `push` to `main`

**Jobs:**

#### Job: `changed` (PR only)
- Runs `make ci-changed` with `FLOTSAM_STRICT=1`
- Fast, risk-aware — only checks what changed

#### Job: `standard` (push to main + PR)
- Setup: Go 1.24, Node 22, pnpm 9
- `make test-standard` (unit tests, lint, web build)
- Env: `FLOTSAM_STRICT=1`, `FLOTSAM_RUN_QUALITY_GATES=1`

#### Job: `coverage`
- Needs: `standard`
- Runs `make coverage`
- Uploads coverage artifacts (Go + web reports)
- Env: `FLOTSAM_STRICT=1`, `FLOTSAM_RUN_QUALITY_GATES=1`

#### Job: `analyzers`
- Needs: `standard`
- Installs available analyzers (gosec, govulncheck, gitleaks)
- Runs `scripts/ci/run-static-analyzers.sh`
- Uploads analyzer output as artifacts
- Env: `FLOTSAM_STRICT=1`

### 2. `.github/workflows/full.yml` — Scheduled / manual full check

**Triggers:**
- `schedule: cron '0 6 * * 1'` (weekly Monday 6am UTC)
- `workflow_dispatch` (manual)

**Jobs:**

#### Job: `full`
- Setup: Go 1.24, Node 22, pnpm 9, PostgreSQL 16 service container (pgvector)
- `make ci-full` with `FLOTSAM_STRICT=1`, `FLOTSAM_RUN_QUALITY_GATES=1`
- `make test-full` (includes integration tests against real DB)
- Upload coverage + analyzer artifacts

---

## Key files

| File | Action |
|---|---|
| `.github/workflows/ci.yml` | **Create** — PR/push workflow |
| `.github/workflows/full.yml` | **Create** — scheduled full workflow |
| `docs/ci/blueprint.md` | **Update** — note that GitHub Actions is now configured |

## Design decisions

1. **Reuse existing scripts** — workflows call `make` targets and `scripts/ci/` scripts directly; no logic duplication
2. **Strict mode on** — both `FLOTSAM_STRICT=1` and `FLOTSAM_RUN_QUALITY_GATES=1` set in all jobs
3. **Analyzer install in CI** — install gosec, govulncheck, gitleaks via `go install` / binary download; semgrep via pip if needed
4. **Service container for full** — PostgreSQL 16 + pgvector for integration tests
5. **Artifacts** — coverage reports and analyzer output uploaded via `actions/upload-artifact@v4`
6. **Caching** — Go module cache + pnpm store cached for speed

## Verification

1. `make test-standard` must pass in the worktree (quality gate)
2. Validate YAML syntax with `python -c "import yaml; yaml.safe_load(open(...))"` or similar
3. Confirm workflows reference correct make targets and script paths
4. Verify env vars are set correctly in each job
