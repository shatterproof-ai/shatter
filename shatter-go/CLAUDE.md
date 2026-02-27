# shatter-go

Go language frontend. Go binary subprocess implementing the JSON-over-stdio protocol.

## Key Files

- `protocol/handler.go` — Protocol handler, `logf()` function that writes `[shatter-go]` lines to stderr
- `instrument/executor.go` — Function execution and instrumentation

## Output Review

After changing stderr logging or protocol output, run `/walkthrough-review` to validate that frontend output respects log-level verbosity.
