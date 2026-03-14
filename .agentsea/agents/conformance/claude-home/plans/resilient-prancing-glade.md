# Toolchain Version Alignment (flt-d56)

## Context
`go.mod` declares `go 1.25.0`, but README, CLAUDE.md, and both CI workflows all say `1.24`. The task says go.mod is authoritative, so everything else aligns to **1.25**.

## Inconsistencies Found

| File | Current | Target |
|---|---|---|
| `api/go.mod` | `go 1.25.0` | (authoritative — no change) |
| `README.md:34` | `Go 1.24+` | `Go 1.25+` |
| `CLAUDE.md:11` | `Go 1.24+` | `Go 1.25+` |
| `.github/workflows/ci.yml:10` | `GO_VERSION: "1.24"` | `GO_VERSION: "1.25"` |
| `.github/workflows/full.yml:9` | `GO_VERSION: "1.24"` | `GO_VERSION: "1.25"` |

## Changes
1. Edit `README.md` — update Go prerequisite from `1.24+` to `1.25+`
2. Edit `CLAUDE.md` — update Go prerequisite from `1.24+` to `1.25+`
3. Edit `.github/workflows/ci.yml` — update `GO_VERSION` from `"1.24"` to `"1.25"`
4. Edit `.github/workflows/full.yml` — update `GO_VERSION` from `"1.24"` to `"1.25"`

## Verification
```bash
make api-test-unit && make api-lint
```
