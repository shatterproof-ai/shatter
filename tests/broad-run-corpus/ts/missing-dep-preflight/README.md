# missing-dep-preflight

Exercises str-jeen.26 — TS preflight on `node_modules`. `package.json`
declares a dependency that is intentionally not installed; preflight should
report a `preflight_failed` status (or equivalent) for the affected file
rather than aborting the broad-run scan.
