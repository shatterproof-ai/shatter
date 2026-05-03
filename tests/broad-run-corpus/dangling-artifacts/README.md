# dangling-artifacts

Exercises str-jeen.4 — artifact references valid. The gate runs
`shatter scan` against this fixture, then walks the JSON / markdown report
and asserts that every absolute path emitted in the output resolves on
disk. Dangling references — for example a path under a temp dir that the
scan cleaned up — fail the gate.
