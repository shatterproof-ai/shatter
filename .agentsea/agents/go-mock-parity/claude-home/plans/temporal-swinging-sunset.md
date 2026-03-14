# Plan: Semgrep Baseline Integration (kapow-3jr.1)

## Context
Add static analysis via Semgrep CE to catch security and correctness issues in Go, TypeScript, and shell code. Semgrep is not currently installed in this environment, so we create config files and documentation that work when semgrep is available.

## Files to Create

### 1. `.semgrep/semgrep.yml` — Main configuration
Semgrep config referencing community rulesets:
- `p/golang` — Go security and correctness
- `p/typescript` — TypeScript patterns
- `p/owasp-top-ten` — OWASP security checks
- `p/secrets` — secret detection

Exclude paths: `node_modules/`, `dist/`, `vendor/`, generated files (`graph/generated/`, `graph/model/models_gen.go`), migration SQL, fixture data.

### 2. `.semgrep/README.md` — Documentation
- Prerequisites (pip install semgrep)
- How to run locally
- CI integration snippet
- Suppression policy: `nosemgrep` inline comments with justification required

### 3. `Makefile` — Add `semgrep` target
Add `semgrep` target that runs `semgrep scan --config .semgrep/semgrep.yml .`
Add to `.PHONY` list. Do NOT add to the `lint` target yet (semgrep not installed everywhere).

### 4. `.gitignore` — Add `.semgrep/.semgrep_logs/` if needed

## Verification
- `make test-quick` passes (config-only, no code changes)
- Config YAML is valid syntax
