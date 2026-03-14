# Plan: ast-grep Structural Rules (kapow-3jr.3)

## Context
The repo enforces several structural policies via CLAUDE.md conventions and one custom ESLint rule. ast-grep can enforce these structurally at the AST level, catching violations that regex-based tools miss. `sg` is already installed at `/usr/bin/sg`.

## Files to Create

### Config
- `.ast-grep/sgconfig.yml` — project config pointing to `rules/`

### Rules (`.ast-grep/rules/`)
1. **`no-bare-mantine-provider.yml`** — Flag `<MantineProvider>` JSX usage outside `AppProviders.tsx`. Tests should use `renderWithMantine()` or `AppProviders`.
2. **`no-raw-graphql-strings.yml`** — Flag template literals containing GraphQL operations (query/mutation/subscription) not wrapped in `graphql()`. Complements the existing ESLint rule with AST-level detection.
3. **`no-fmt-sprintf-sql.yml`** — Flag `fmt.Sprintf` calls where the format string contains SQL keywords (SELECT, INSERT, UPDATE, DELETE, WHERE). Go rule.
4. **`no-tailwind-classes.yml`** — Flag `className` props containing common Tailwind utility patterns (flex, grid, p-N, m-N, bg-, text-, etc.) in TSX files.

### Test Fixtures (`.ast-grep/tests/`)
Each rule gets a pair of fixture files (positive match / negative match) for `sg test`.

### Documentation
- `.ast-grep/README.md` — Rule inventory, how to run, CI integration notes.

## Integration
- Add `ast-grep-check` target to root Makefile: `sg scan --rule .ast-grep/rules/`
- Wire into `make lint` target

## Verification
```bash
sg test                    # all rule tests pass
sg scan                    # runs against codebase, no false positives
make test-quick            # existing checks still pass
```
