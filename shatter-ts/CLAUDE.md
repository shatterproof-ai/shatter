# shatter-ts

TypeScript language frontend. Node.js subprocess implementing the JSON-over-stdio protocol.

## Key Files

- `src/main.ts` — Entry point, protocol handler, `log()` function that writes `[shatter-ts]` lines to stderr

## Output Review

After changing stderr logging or protocol output, run `/walkthrough-review` to validate that frontend output respects log-level verbosity.
