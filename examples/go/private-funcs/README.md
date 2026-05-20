# private-funcs (str-z06h)

Fixture for Shatter's opt-in unexported Go function discovery policy.

`ClassifyAmount` is the only exported symbol. `isSmall` and `bucket` are
unexported helpers in the same package.

## Default scan (private functions omitted)

```
shatter scan examples/go/private-funcs
```

Only `ClassifyAmount` is explored. The scan summary records that two
unexported functions were intentionally skipped.

## Opt-in scan (private functions included)

```
shatter scan --all examples/go/private-funcs
```

All three functions are explored; `bucket`'s switch arms contribute
additional branch coverage to the report.

The opt-in is documented in `docs/go-frontend-scope-limits.md`. Calling
unexported functions across packages remains out of scope.
