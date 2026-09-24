# Budget-allocation benchmark

Corpus: `/var/tmp/zolem-bench`. Seeds: [1, 2, 3]. Per-function flat budget: 100 paths × 5 executions.

## Totals (median over seeds)

| arm | Σ branches covered / total | Σ lines covered / total | Σ executions used | Σ allocated | Σ claimed | wall s |
|---|---|---|---|---|---|---|
| flat | 33 / 43 | 167 / 353 | 3632 | 0 | 0 | 28.4 |
| static | 33 / 43 | 167 / 353 | 3628 | 12000 | 0 | 28.1 |

## Paired static − flat (per seed)

| metric | n | median Δ | per-seed |
|---|---|---|---|
| branches_covered | 3 | +0.0 | +0.0, +0.0, +0.0 |
| lines_covered | 3 | +0.0 | +0.0, +0.0, +0.0 |
| iterations | 3 | +0.0 | +0.0, +0.0, -4.0 |
| wall_s | 3 | -0.4 | +0.2, -1.1, -0.4 |

## Outcomes (seed 1)

| arm | behavioral | error_only |
|---|---|---|
| flat | 15 | 9 |
| static | 15 | 9 |

## Per function (seed 1, static allocation descending)

| function | allocated | claimed | executions flat/static | branches flat/static | lines flat/static |
|---|---|---|---|---|---|
| internal/specs/discovery.go::NormalizeGeminiDiscovery | 1106 | 0 | 233/233 | 4/4 | 16/16 |
| internal/specs/fetcher.go::(*Fetcher).Get | 1062 | 0 | 231/231 | 4/4 | 13/13 |
| internal/specs/openapi.go::NormalizeOpenAPI | 1023 | 0 | 232/232 | 3/3 | 11/11 |
| internal/specs/openapi.go::(OpenAPINormalizer).Normalize | 965 | 0 | 234/234 | 3/3 | 10/10 |
| internal/specs/validator.go::(*Validator).Validate | 963 | 0 | 231/231 | 1/1 | 7/7 |
| internal/specs/validator.go::(*Validator).LoadRaw | 905 | 0 | 231/231 | 1/1 | 5/5 |
| internal/specs/loader.go::(PassThroughNormalizer).Normalize | 764 | 0 | 245/245 | 3/3 | 14/14 |
| internal/specs/discovery.go::LoadProviderSchema | 759 | 0 | 387/387 | 3/3 | 12/12 |
| internal/specs/loader.go::(*ContractLoader).Refresh | 555 | 0 | 233/233 | 3/3 | 11/11 |
| internal/specs/fetcher.go::NewFetcherWithFallback | 538 | 0 | 21/21 | 0/0 | 2/2 |
| internal/specs/validator.go::(*Validator).LoadNormalized | 525 | 0 | 231/231 | 1/1 | 6/6 |
| internal/specs/loader.go::(*ContractLoader).LoadFallback | 425 | 0 | 231/231 | 1/1 | 5/5 |
| internal/specs/registry.go::(Registry).List | 337 | 0 | 231/231 | 3/3 | 12/12 |
| internal/specs/registry.go::DefaultRegistry | 312 | 0 | 231/231 | 1/1 | 6/6 |
| internal/specs/validator.go::(*Validator).Has | 312 | 0 | 21/21 | 0/0 | 6/6 |
| internal/specs/registry.go::(Registry).Lookup | 306 | 0 | 231/231 | 1/1 | 8/8 |
| internal/specs/fetcher.go::NewFetcher | 241 | 0 | 21/21 | 0/0 | 3/3 |
| internal/specs/validator.go::(*Validator).Schemas | 212 | 0 | 21/21 | 1/1 | 8/8 |
| internal/specs/loader.go::NewContractLoader | 204 | 0 | 21/21 | 0/0 | 2/2 |
| internal/specs/validator.go::(*ValidationError).Error | 138 | 0 | 21/21 | 0/0 | 2/2 |
| internal/specs/vendored.go::VendoredFallbacks | 99 | 0 | 21/21 | 0/0 | 2/2 |
| internal/specs/registry.go::(ContractSource).HasRemote | 83 | 0 | 21/21 | 0/0 | 2/2 |
| internal/specs/registry.go::(ContractSource).Key | 83 | 0 | 21/21 | 0/0 | 2/2 |
| internal/specs/validator.go::NewValidator | 83 | 0 | 21/21 | 0/0 | 2/2 |

Sanity gate (flat == knob-absent, seed 1): PASS.
