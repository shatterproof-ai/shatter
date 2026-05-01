# mixed-package

Mixed-package regression fixture for **str-jeen.35** (cross-linked **str-x0sv**).

## Shape

| File | Package | Role |
| --- | --- | --- |
| `admissions.go` | `main` | Target source. Carries `Compute(int) string` and a no-op `main()`. |
| `admissions_heuristic_test.go` | `main` | Internal `_test.go` sibling — same package as the target. |
| `admissions_external_test.go` | `main_test` | External `_test.go` sibling. |
| `go.mod` | — | Self-contained module `example.com/mixedpkg`. |

## What it exercises

The shatter-go build pipeline overlays the target source with a rewritten
copy declared `package shattertarget` before invoking `go build -overlay`.
Without the str-x0sv fix the directory then contains both `shattertarget`
(rewritten target) and `main` (untouched `_test.go` siblings), and the
loader rejects it with:

```
found packages shattertarget (admissions.go) and main (admissions_heuristic_test.go)
```

The fix stages overlay-rewritten copies of every `_test.go` sibling so the
directory view stays internally consistent.

## Regression test

`shatter-go/build/builder_mixedpkg_fixture_test.go` (build tag
`integration`) drives `build.Builder` against this fixture and asserts the
launcher binary builds. It would fail on the str-x0sv regression with a
`found packages` diagnostic.
