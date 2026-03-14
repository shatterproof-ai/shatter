# Plan: Semgrep Custom Policy Rules (kapow-3jr.2)

## Context

The project has `rules: []` in `.semgrep/semgrep.yml` and relies on community packs via Makefile flags. ast-grep already covers some patterns (fmt.Sprintf SQL injection with quoted values, `gql` template literals, Tailwind classes, bare MantineProvider). ESLint covers raw GraphQL strings. Semgrep rules should **complement** these existing tools, catching patterns they miss or providing defense-in-depth.

## Rules to Implement

### 1. `kapow-no-fmt-sprintf-sql` (Go, ERROR)
**Complements ast-grep**: ast-grep catches `fmt.Sprintf` with quoted `%s` in SQL. Semgrep can catch broader patterns like string concatenation (`+`) to build SQL, and `fmt.Sprintf` with unquoted `%s` in WHERE/AND clauses.
- Pattern: `fmt.Sprintf("...", ...)` where string contains SQL keywords
- Metavariable-regex on the format string
- Exclude: test files, migrations, generated code

### 2. `kapow-no-string-concat-sql` (Go, ERROR)
Catches SQL built via string concatenation (`"SELECT ... " + variable`).
- Pattern-either with common SQL keywords
- Exclude: test files, `internal/search/` (SQL builder by design)

### 3. `kapow-no-fmt-println-api` (Go, WARNING)
Project mandates `slog` for structured logging. Catch `fmt.Println`, `fmt.Printf`, `fmt.Print` in API code.
- Paths: include `api/`
- Exclude: test files, `_test.go`

### 4. `kapow-no-raw-graphql-template` (TypeScript, ERROR)
Defense-in-depth alongside ESLint. Catch raw template literals containing `query`/`mutation`/`subscription` keywords.
- Pattern: template literal or string assigned to variable matching GraphQL operation syntax
- Exclude: `node_modules`, `dist`, test files, `*.test.*`

### 5. `kapow-no-direct-urql-import` (TypeScript, ERROR)
Project requires importing `useQuery`/`useMutation` from `@/lib/graphql`, not directly from `urql`.
- Pattern: `import { useQuery } from 'urql'` and variants
- Exclude: `lib/graphql.ts` itself (the wrapper)

### 6. `kapow-no-hardcoded-hex-color` (TypeScript, WARNING)
Detect hardcoded hex color codes in style props or color attributes in TSX.
- Pattern-regex on strings matching `#[0-9a-fA-F]{3,8}` in style/color contexts
- Exclude: `theme.ts`, `index.css` (where colors are defined), test files

### 7. `kapow-no-sql-in-log` (Go, ERROR)
Prevent logging SQL queries which could contain sensitive data.
- Patterns: `slog.Info/Error/Warn/Debug` with "query" or "sql" key containing a variable
- Exclude: test files

## Files to Modify
- `.semgrep/semgrep.yml` — Add all rules

## Verification
1. Validate YAML: `python3 -c "import yaml; yaml.safe_load(open('.semgrep/semgrep.yml'))"`
2. Try running: `semgrep scan --config .semgrep/semgrep.yml . --exclude='node_modules' --exclude='dist' --exclude='api/graph/generated' 2>&1 | head -80`
3. Commit and push branch
