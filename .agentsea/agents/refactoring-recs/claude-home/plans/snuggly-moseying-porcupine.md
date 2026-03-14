# Plan: Semgrep CE Starter Rules + Local Hook Bootstrap

## Context

Two issues from the "Process guardrails rollout" epic (str-28vd):
- **str-28vd.2**: Add Semgrep CE config with starter rules for protocol/frontend parity and config boundary duplication.
- **str-28vd.6**: Add hook bootstrap so pre-commit/pre-push call repo-owned quality scripts.

The repo already has:
- `scripts/quality/check-meta.sh` with Semgrep support wired (looks for `.semgrep/shatter.yml`, runs `semgrep` if installed)
- Beads-managed git hooks (pre-commit, pre-push) that delegate to `bd hooks run`
- `scripts/quality/check-all.sh` orchestrating all quality gates
- `scripts/setup-dev.sh` that bootstraps Beads hooks

## Task #4: Semgrep CE Starter Rules (str-28vd.2)

### Branch: `str-28vd.2`

### Files to create

1. **`.semgrep/shatter.yml`** — Starter ruleset with two categories:

   **Rule 1: Protocol/frontend parity — env var coverage**
   - Pattern: detect `SHATTER_EXEC_TIMEOUT` string literal usage — ensure it appears in all three frontends (Go, TS, Rust). This is a "grep-lint" style rule.
   - Implementation: Use `pattern-regex` to flag files that reference `SHATTER_EXEC_TIMEOUT` but miss the env-var read pattern. Actually, Semgrep works per-file so cross-file parity is hard. Better approach: a rule that flags hardcoded timeout defaults without referencing the env var constant — e.g., a raw numeric timeout literal in executor files.

   **Better Rule 1: Hardcoded timeout defaults**
   - In executor/handler files, flag raw numeric literals used as timeout defaults (magic numbers) without a named constant. Use `metavariable-pattern` on numeric literals in timeout-related contexts.

   **Revised Rule 1: `unwrap()` in library code**
   - CLAUDE.md says "No `unwrap()` in Rust library code." This is a perfect, high-signal Semgrep rule for Rust files in `shatter-core/src/`.
   - Pattern: `.unwrap()` in `shatter-core/src/**/*.rs` (excluding test files).

   **Rule 2: Duplicated config boundaries — hardcoded absolute paths**
   - CLAUDE.md says "No hardcoded absolute paths." Flag string literals starting with `/home/`, `/tmp/`, `/usr/` etc. in source files.
   - Pattern: `pattern-regex` matching `"/home/...` or `"/tmp/...` string literals.

   **Rule 3 (bonus): Protocol parity reminder**
   - Flag `SHATTER_EXEC_TIMEOUT` in new files that don't have the env-var read pattern — helps ensure new frontends implement the timeout contract.

### Files to modify

2. **No changes needed to `check-meta.sh`** — it already reads `SEMGREP_CONFIG` (defaulting to `.semgrep/shatter.yml`) and runs Semgrep when available. Just creating the config file is sufficient.

### Verification
- `semgrep --config .semgrep/shatter.yml .` (if semgrep installed)
- `bash scripts/quality/check-meta.sh` (should find and use the config)

---

## Task #5: Local Hook Bootstrap (str-28vd.6)

### Branch: `str-28vd.6`

### Design

The existing Beads hooks (pre-commit, pre-push) are managed by `bd hooks install` and only run `bd hooks run`. The goal is to also call repo-owned quality scripts from hooks, without duplicating logic.

**Approach**: Create a `scripts/setup-hooks.sh` that appends repo-quality-gate calls to the git hooks (after the Beads section). The hooks call `scripts/quality/` entrypoints directly.

### Files to create

1. **`scripts/setup-hooks.sh`** — Idempotent script that:
   - Creates `.git/hooks/pre-commit` and `.git/hooks/pre-push` if missing
   - Appends a "SHATTER QUALITY" section (guarded by markers) that calls:
     - pre-commit: `scripts/quality/check-rust.sh` (quick lint, clippy) — or just a lightweight subset
     - pre-push: `scripts/quality/pre-completion.sh` (full quality gates)
   - Preserves existing Beads hook sections
   - Is idempotent (checks for markers before appending)

2. **`docs/hooks.md`** — Brief documentation explaining:
   - How to set up hooks (`scripts/setup-hooks.sh`)
   - What each hook runs
   - How to customize/skip (e.g., `--no-verify` escape hatch mention)

### Files to modify

3. **`scripts/setup-dev.sh`** — Add a call to `scripts/setup-hooks.sh` after Beads hook installation (around line 236), so hook bootstrap happens automatically during dev setup.

### Verification
- Run `scripts/setup-hooks.sh` and verify hooks are updated
- Check `.git/hooks/pre-commit` contains both Beads and Shatter sections
- A test commit triggers the quality scripts

---

## Execution Order

1. `bd start str-28vd.2` → create `.semgrep/shatter.yml` → commit → push
2. `bd start str-28vd.6` → create `scripts/setup-hooks.sh` + `docs/hooks.md` → update `setup-dev.sh` → commit → push
