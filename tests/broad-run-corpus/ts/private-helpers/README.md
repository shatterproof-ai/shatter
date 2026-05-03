# private-helpers

Kapow-class fixture: TypeScript modules where a public exported function
delegates to an unexported helper. The analyzer should target the public
function only; the private helper should not appear as an independent scan
target (str-jeen.9 / kapow TS regression class).

Used by the broad-run validation corpus gate
(`scripts/broad_run_validation_gate.py`). See
`tests/broad-run-corpus/manifest.yaml` for thresholds.
