# Plan: Config Parity Checker (kapow-n92.5)

## Context

Config fields are declared in Go (`api/internal/config/config.go` with `env:"..."` tags), documented in `.env.example`, and used across the codebase. There's no automated check for drift between these three sources. This script catches: undocumented config, documented-but-not-declared vars, and dead config fields.

## Deliverables

1. `scripts/check-config-parity.sh` — the checker script
2. `scripts/config-parity-waivers.txt` — known exceptions

## Design

### Extraction

**Config fields** — grep `env:"..."` tags from `api/internal/config/config.go`, extract var names:
```bash
grep -oP 'env:"([A-Z_]+)' api/internal/config/config.go | sed 's/env:"//'
```

**Documented vars** — extract uncommented `KEY=value` and commented `# KEY=value` lines from `.env.example`. Only lines matching `^[A-Z_]+=` or `^# [A-Z_]+=`. Skip pure comment lines (no `=`), section headers, and OTEL SDK vars (not in our config struct).
```bash
grep -oP '^#?\s*[A-Z][A-Z_]*(?==)' .env.example | sed 's/^#\s*//'
```

**Runtime usage** — for each config field, check if the Go field name (e.g., `DatabaseURL`) appears in `api/` Go files outside of `config.go` and `*_test.go`. The field name maps from the env var via Go naming convention (`DATABASE_URL` → `DatabaseURL`). Simpler: grep for `cfg.FieldName` or `.FieldName` patterns in `api/`.

Actually, the simplest reliable approach: for each env var name, grep for it literally in the codebase (it appears in struct tags, .env.example, docs). For runtime usage, we need to check the Go *field name* is accessed somewhere. But deriving field names from env var names requires knowing the Go naming convention, which is fragile.

Better approach for "used at runtime": grep for the env var name in Go files outside config.go. The env var name appears in the struct tag, so any file importing config and accessing the field will reference the struct. But the field names don't contain the env var names directly.

Simplest reliable approach: check that config fields are referenced by their Go field name (e.g., `DatabaseURL`, `JWTSecret`) somewhere in `api/` outside `config.go`. We can extract both the env var name and Go field name from the same grep of config.go.

### Waivers

A simple text file with one env var name per line (comments with `#`). Waived vars skip all checks.

### .env.example vars not in Config struct

Some `.env.example` vars are frontend-only (`VITE_SUPABASE_URL`, `VITE_SUPABASE_ANON_KEY`, `SUPABASE_URL`, `SUPABASE_ANON_KEY`) or belong to external SDKs (`OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_EXPORTER_OTLP_HEADERS`, `OTEL_SERVICE_NAME`). These should be waived.

### Script structure

```
1. Parse config.go → array of (ENV_VAR, GO_FIELD) pairs
2. Parse .env.example → set of documented env var names
3. Load waivers
4. Check 1: config fields missing from .env.example
5. Check 2: .env.example vars missing from config struct
6. Check 3: config fields not used at runtime (dead config)
7. Print summary, exit non-zero on errors
```

### Output style

Match existing scripts (colored PASS/ERROR/WARN, summary line).

## Files to create/modify

- **Create** `scripts/check-config-parity.sh`
- **Create** `scripts/config-parity-waivers.txt`

## Waivers file contents

```
# Frontend-only vars (used by Vite/Supabase JS SDK, not the Go API)
SUPABASE_URL
SUPABASE_ANON_KEY
VITE_SUPABASE_URL
VITE_SUPABASE_ANON_KEY

# OpenTelemetry SDK env vars (consumed by OTel SDK, not our Config struct)
OTEL_EXPORTER_OTLP_ENDPOINT
OTEL_EXPORTER_OTLP_HEADERS
OTEL_SERVICE_NAME
```

## Verification

```bash
cd /path/to/worktree
bash scripts/check-config-parity.sh
# Should exit 0 with all checks passing
```
