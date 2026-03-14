# Fix Pre-existing Test Failures

## Context

`make test-standard` has two pre-existing failures unrelated to recent work:
1. **Supabase test** — env var leakage from `.env` into Vitest
2. **golangci-lint** — v1 binary installed but config is v2 format

Both break the CI quality gate and must be fixed for `make test-standard` to pass.

---

## Fix 1: Supabase test env leakage

**Root cause**: Vite auto-loads `.env` (which has real `VITE_SUPABASE_URL` and
`VITE_SUPABASE_ANON_KEY`) into `import.meta.env`. The test at
`web/src/lib/supabase.test.ts` tries to test the "no env vars" path but the
vars are already injected.

**Fix**: Use `vi.stubEnv()` (Vitest's built-in API) to control env vars per test,
instead of manually deleting/setting properties on `import.meta.env`.

### Changes

**`web/src/lib/supabase.test.ts`**:
- In the "no-op stub" test: call `vi.stubEnv('VITE_SUPABASE_URL', '')` and
  `vi.stubEnv('VITE_SUPABASE_ANON_KEY', '')` before the dynamic `import('./supabase')`
- In the "creates real client" test: call `vi.stubEnv('VITE_SUPABASE_URL', 'https://test.supabase.co')`
  and `vi.stubEnv('VITE_SUPABASE_ANON_KEY', 'test-anon-key')` before the dynamic import
- Add `vi.unstubAllEnvs()` in `afterEach` for cleanup
- Remove manual `import.meta.env` property manipulation

---

## Fix 2: golangci-lint version mismatch

**Root cause**: `.devcontainer/Dockerfile` installs golangci-lint v1.64.8, but
`api/.golangci.yml` declares `version: "2"`. The config uses zero v2-specific
features — just 5 enabled linters and directory exclusions.

**Fix**: Upgrade golangci-lint to v2 in the Dockerfile. The config already works
with v2; no config changes needed. v1 is legacy.

### Changes

**`.devcontainer/Dockerfile`** (line 16):
- Change version from `v1.64.8` to latest stable v2 (e.g. `v2.1.6` or whatever
  `curl -sSfL ... | sh -s -- -b ... latest` resolves to)

---

## Verification

```bash
# After both fixes:
cd web && pnpm test -- src/lib/supabase.test.ts   # supabase tests pass
make api-lint                                       # golangci-lint runs clean
make test-standard                                  # full gate passes
```
