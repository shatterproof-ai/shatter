# browser-globals

Exercises str-jeen.30 — TS browser-global handling. The fixture uses
`window`, `document`, and `fetch` (host globals). The analyzer must accept
the references without crashing; broad-run reports must classify these
targets coherently rather than as analyze failures.
