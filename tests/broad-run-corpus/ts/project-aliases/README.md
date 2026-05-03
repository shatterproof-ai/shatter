# project-aliases

Exercises str-jeen.27 / str-jeen.28 — TS project-reference tsconfig and path
alias resolution. `src/index.ts` imports `@util/clamp` via the alias
declared in `tsconfig.json:paths`; without alias support the analyzer reports
the import as missing.
