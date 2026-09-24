# Maintainer decisions on revised drafts (2026-09-23)

Applied to the drafts after the Codex-driven revision. D1-D6 are recorded in the report's
"Maintainer decisions (2026-09-23)" section.

- **Accepted reviser positions (no objection):** the concolic stop-reason loss is in the CLI
  `ExploreResultAccumulator`, not `orchestrator.rs:2557`; str-0z1im is closed, so there is no
  overlap; `validator-reopen-note` stays a comment-only reopen-note.
- **D7 revalidate ExpectedDrift:** fails the exit code by default. A flag overrides it (for example
  `--allow-drift`), and so does a config equivalent. Drift is still reported in its own count.
- **D8 sandbox backend:** confirmed fail-closed. With only `SHATTER_SANDBOX_BACKEND` set, TS and Rust targets are refused with a message that the backend is Go-only. An unknown backend value is an error.
- **D9 coverage headline:** **branch** coverage is the named headline for explore, scan and run. Line coverage is a named secondary line. `coverage-headline-metric-unification` is therefore blocked by `branch-metric-counts-sites`. run and scan share one default iteration budget.
- **D10 require flag:** `--require-frontend <lang>` (repeatable; doctor, scan and run; exit 2). A note on str-qwua7.40 retires `--require-rust`.
