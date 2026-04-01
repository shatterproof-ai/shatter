# CI Integration Guide

## Purpose

This repository uses [Taskfile](https://taskfile.dev/) as the build and quality
orchestration layer. All quality gates are defined as Taskfile tasks and serve as
the stable interface between local development, hooks, agent workflows, and CI.

The CI platform is intentionally unspecified. The `npx task` commands should
therefore be treated as the canonical entrypoints, regardless of whether the
eventual runner is GitHub Actions, Buildkite, Woodpecker, Jenkins, or another
system.

## Task Inventory

### General

- `npx task tooling`
  - reports required and optional tool availability
- `npx task check`
  - aggregate quality runner
- `npx task pre-completion`
  - local completion gate with git status summary

### Language-specific

- `npx task core:test` / `npx task core:clippy`
- `npx task ts:test`
- `npx task go:test`

### Docs and repository metadata

- `npx task docs`
- `npx task meta`

### Protocol validation

- `npx task schemas`
- `npx task conformance`

## Strict vs Permissive Mode

Tasks operate in two modes, controlled by environment variables or flags:

| Mode | Behavior when an optional tool is missing |
|---|---|
| **Permissive** (default) | Print `[skip]` and continue — the check is silently omitted |
| **Strict** | Fail with a non-zero exit code |

**When to use each:**

- **Local development / agent workflows**: permissive (default). Developers and
  agents may not have every optional analyzer installed. Skipping is acceptable
  because the CI pipeline catches anything missed locally.
- **CI pipelines**: strict. Once the CI image includes a tool, its absence is a
  signal that the image is broken — fail fast.

## Recommended CI Stage Layout

### Stage 1: Fast repository checks

Purpose:

- fail quickly on obvious repo-level issues

Recommended commands:

```bash
npx task tooling
npx task docs
npx task meta
```

### Stage 2: Language quality gates

Purpose:

- run the standard language-specific checks in parallel when possible

Recommended commands:

```bash
npx task core:test
npx task core:clippy
npx task ts:test
npx task go:test
```

Notes:

- TypeScript tasks currently run build plus tests. Dependency-cruiser and
  Knip should be added later once their configs are committed.
- Rust tasks cover both the workspace crates (`shatter-core`,
  `shatter-cli`) and the standalone Rust frontend crates (`shatter-rust`,
  `shatter-rust-runtime`) which are excluded from the Cargo workspace. Tests and
  clippy run for all four crates.

### Stage 3: Deep or slow checks

Purpose:

- run heavier gates only when the relevant parts of the repo change

Recommended commands:

```bash
npx task e2e
npx task walkthrough
```

Notes:

- Treat this as a separate job or nightly gate if runtime becomes expensive.
- The walkthrough should remain outside the aggregate check because it is a
  substantially slower end-to-end workflow.

## Recommended CI Modes

### Pull request / merge request

Use:

```bash
npx task check
```

Add E2E if the change touches:

- solver logic
- explorer/orchestrator behavior
- protocol types
- frontend instrumentation
- CLI wiring for explorer modes

```bash
npx task e2e
```

### Main-branch protection

Use:

```bash
npx task check
npx task e2e
```

Optionally add:

```bash
npx task walkthrough
```

### Nightly or scheduled validation

Use the full quality stack, including:

- aggregate checks
- E2E tests
- walkthrough
- future vulnerability scans and dependency freshness checks

### Weekly perf tracking

Run the dedicated perf workflow on a fixed schedule or manually.

Recommended behavior:

- execute the stable scenario subset
- write artifacts to an untracked workspace directory
- upload the full result bundle as a CI artifact
- optionally compare against a curated baseline when one is configured
- never commit routine perf runs back into the repository

## Git Submodules

The repository uses git submodules for shared content:

| Submodule | Path | Purpose |
|---|---|---|
| `agents` | `.agents/` | Shared agent rules and tooling |
| `examples` | `examples/` | Example target programs for testing, demos, and E2E fixtures |

### CI checkout requirements

CI jobs must initialize submodules before running any tasks. Either:

```bash
# Option A: clone with submodules
git clone --recurse-submodules <repo-url>

# Option B: initialize after clone (useful for shallow clones)
git submodule update --init

# Option C: shallow submodules (faster for CI)
git clone --recurse-submodules --shallow-submodules <repo-url>
```

All test tiers, the walkthrough, and the E2E suite require the `examples/` submodule. If `examples/` is empty, tests will fail with file-not-found errors.

### Updating the examples submodule

When examples are updated upstream, update the pinned commit:

```bash
git submodule update --remote examples
git add examples
git commit -m "chore: update examples submodule"
```

## Tool Installation Expectations

### Required baseline tools

The tasks assume the CI image provides:

- `bash`
- `git`
- `cargo`
- `node`
- `npm`
- `go`

### Optional analyzers for strict CI mode

To enable strict CI mode fully, install:

- `semgrep`
- `actionlint`
- `lychee`
- `vale`
- `markdownlint-cli2`
- `golangci-lint`
- `staticcheck`
- `govulncheck`
- `cargo-deny` (configured — `deny.toml` checked in)
- optional later: `cargo-nextest`, `cargo-udeps`, `reviewdog`

## CI Design Rules

### 1. Keep CI YAML thin

The CI configuration should select stages and install tools, but the actual
check logic should remain in Taskfile tasks.

### 2. Prefer parallel jobs by domain

Recommended split:

- docs/meta
- Rust
- TypeScript
- Go
- deep/E2E
- scheduled perf

### 3. Fail on missing tools only in controlled environments

Locally, skipped optional tools are acceptable. In CI, strict mode should be
used once the CI image is provisioned with the expected analyzer set.

### 4. Do not fork local and CI logic

If a check is important enough for CI, it should have a Taskfile task
entrypoint that developers and agents can also run locally.

## Hook Integration Guidance

The same tasks should be used by local hooks:

- `pre-commit`
  - `npx task core:clippy`
  - `npx task docs`
  - `npx task meta`
- `pre-push`
  - `npx task check`

Avoid copying the underlying commands directly into hook configs. Call the
tasks instead.

## Agent Workflow Guidance

The same tasks should back Claude/agent workflows:

- bugfix workflow
  - targeted failing test first
  - targeted rerun
  - `npx task pre-completion`
- review workflow
  - targeted check task(s) for the area under review
- completion workflow
  - `npx task pre-completion`

## Go Analysis Tools

Three Go-specific analysis tools are available via `npx task go:test`. Each serves a
different purpose and should be enabled at the appropriate CI stage.

### golangci-lint

**Config**: `shatter-go/.golangci.yml` (v2 format)

**What it checks**: style, correctness, and performance issues via a curated set
of linters (errcheck, govet, staticcheck, gocritic, misspell, and others).

**When to enable**: always — every PR and main-branch build. This is the primary
Go linting tool and catches the broadest class of issues.

### staticcheck

**What it checks**: advanced static analysis (SA-class diagnostics). Many checks
overlap with golangci-lint's built-in staticcheck linter, but running standalone
ensures the full diagnostic set is applied.

**When to enable**: every PR and main-branch build, same as golangci-lint. The
marginal cost is low and it catches subtle bugs that other linters miss.

### govulncheck

**What it checks**: known vulnerabilities in Go dependencies by consulting the
Go vulnerability database.

**When to enable**: main-branch protection and nightly builds. Skip on PRs
unless `go.mod` or `go.sum` changed — vulnerability checks are slow and only
change when dependencies change.

## Current Limitations

**Configured tools** (config checked in, ready for CI):

- `cargo-deny` — dependency policy in `deny.toml` (licenses, advisories, sources)

**Not yet configured:**

- `Semgrep CE` is not yet configured in-repo
- docs lint tools may be skipped locally when missing
- generated CLI-doc freshness checks are not yet implemented
- hook configuration is not yet committed

Those items should be added as follow-up issues.
