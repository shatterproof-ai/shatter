# str-28vd.4: Docs Quality Tool Configs

## Context

The `scripts/quality/check-docs.sh` script already invokes markdownlint-cli2, Vale, and lychee — but no config files exist, so tools run with defaults (or skip if not installed). This task adds project-specific configs so the tools catch real issues when available.

## Files to Create

### 1. `.markdownlint-cli2.yaml` (repo root)

Markdownlint config tailored to this project:
- Disable MD013 (line length) — many docs have long table rows and code blocks
- Disable MD033 (inline HTML) — not used but avoid false positives
- Disable MD041 (first-line-h1) — sub-crate CLAUDE.md files start with `# shatter-*` which is fine, but some files may not
- Enable all other defaults
- Glob pattern to cover `**/*.md` but ignore `node_modules`, `target`, `.claude`

### 2. `.vale.ini` (repo root) + `styles/Shatter/` directory

Vale config:
- `MinAlertLevel = suggestion`
- Use built-in `Vale` style (ships with vale, no download needed)
- Create a minimal `styles/Shatter/` vocab with project terms (Shatter, concolic, proptest, Z3, FFI, etc.) so Vale doesn't flag them as spelling errors
- Scope to `*.md` files
- Ignore `node_modules`, `target`, `.claude`

### 3. `lychee.toml` (repo root)

Lychee link checker config:
- Exclude `localhost` / `127.0.0.1` URLs
- Exclude known-flaky external domains if any
- Set timeout to 30s
- Accept `200..=299` status codes
- Ignore `target/`, `node_modules/`, `.claude/`

### 4. Update `scripts/quality/check-docs.sh`

Minimal changes:
- Pass `--config .markdownlint-cli2.yaml` explicitly to markdownlint-cli2 (it auto-detects, but being explicit is clearer)
- Pass `--config lychee.toml` to lychee
- Vale reads `.vale.ini` automatically — no change needed
- Add `PLAN.md SPEC.md PROTOCOL.md PARITY.md` to `DOC_TARGETS` array (currently only checks README, AGENTS, CLAUDE, and docs/)

## Verification

1. Run `bash scripts/quality/check-docs.sh` from worktree — should complete with `[skip]` for missing tools, no errors
2. If markdownlint-cli2 is available: verify it catches at least one real issue (e.g., trailing whitespace, inconsistent list markers)
3. Script should still work gracefully when tools are not installed

## Files Modified
- `scripts/quality/check-docs.sh` — add targets, explicit config flags
- `.markdownlint-cli2.yaml` — new
- `.vale.ini` — new
- `styles/Shatter/accept.txt` — new (Vale vocabulary)
- `lychee.toml` — new
