# source-churn

Two-phase source-churn fixture (str-jeen.3 — run manifest source snapshot).

The gate driver:

1. Copies `initial/` into a tempdir; runs `shatter scan` (phase 1) and
   captures the JSON report.
2. Copies `added-file/extra.go` into the tempdir; runs `shatter scan`
   again (phase 2) against the same workdir.
3. Asserts that phase 2's discovered function set is a strict superset
   of phase 1's (i.e. the new file's targets show up) — proving that the
   scan re-detects files added between runs.

Sequencing is in the gate, not in wall-clock waiting; no `time.sleep`.
