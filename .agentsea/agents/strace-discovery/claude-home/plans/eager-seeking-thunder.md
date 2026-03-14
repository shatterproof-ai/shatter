# Plan: Lefthook Integration (kapow-c0s.1)

## Context

The repo lacks shared git hook orchestration. Developers and agents run ad-hoc
checks (or none) before committing/pushing, leading to inconsistent quality.
Lefthook provides fast, repo-local, declarative hook configuration that works
for both humans and automation.

## Changes

### 1. Create `lefthook.yml` at repo root

```yaml
pre-commit:
  parallel: true
  commands:
    go-vet:
      root: "api/"
      run: go vet ./...
    web-lint:
      root: "web/"
      run: pnpm lint

pre-push:
  commands:
    test-quick:
      run: make test-quick
```

- **pre-commit** runs `go vet` and `pnpm lint` in parallel (~5-10s)
- **pre-push** runs `make test-quick` which covers Go unit tests + TS build + ESLint (~10-20s)

### 2. Update `README.md` — add Git Hooks section

Add a section after the "CI" section documenting:
- What Lefthook is and why it's used
- How to install (`go install` or `npm i -g`)
- How to activate (`lefthook install`)
- How to skip when needed (`--no-verify`)

### 3. Update `.gitignore` if needed

Check if `.gitignore` already covers Lefthook's local override file (`lefthook-local.yml`).
If not, no action needed — it's optional and typically not gitignored.

## Files Modified

| File | Action |
|---|---|
| `lefthook.yml` | Create — hook configuration |
| `README.md` | Edit — add Git Hooks section |

## Verification

```bash
make test-quick   # config-only change, quick gate suffices
```
