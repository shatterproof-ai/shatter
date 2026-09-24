# Budget-allocation benchmark

Corpus: `/var/tmp/zolem-bench`. Seeds: [1, 2, 3]. Per-function flat budget: 10 paths × 5 executions.

## Totals (median over seeds)

| arm | Σ branches covered / total | Σ lines covered / total | Σ executions used | Σ allocated | Σ claimed | wall s |
|---|---|---|---|---|---|---|
| flat | 31 / 43 | 161 / 353 | 910 | 0 | 0 | 24.4 |
| static | 31 / 43 | 161 / 353 | 1103 | 1200 | 0 | 24.4 |

## Paired static − flat (per seed)

| metric | n | median Δ | per-seed |
|---|---|---|---|
| branches_covered | 3 | +0.0 | +0.0, +0.0, +0.0 |
| lines_covered | 3 | +0.0 | +0.0, +0.0, +0.0 |
| iterations | 3 | +193.0 | +193.0, +193.0, +193.0 |
| wall_s | 3 | +0.1 | +0.1, -0.2, +0.8 |

## Per seed (static vs flat)

| seed | Δ branches | Δ lines | Δ wall s | claimed (static) | functions lost | functions gained | Σ allocated == n×budget |
|---|---|---|---|---|---|---|---|
| 1 | +0 | +0 | +0.1 | 0 | 0 | 0 | yes |
| 2 | +0 | +0 | -0.2 | 0 | 0 | 0 | yes |
| 3 | +0 | +0 | +0.8 | 0 | 0 | 0 | yes |

## Outcomes (seed 1)

| arm | behavioral | error_only |
|---|---|---|
| flat | 15 | 9 |
| static | 15 | 9 |

## Per function (seed 1, static allocation descending)

| function | allocated | claimed | executions flat/static | branches flat/static | lines flat/static |
|---|---|---|---|---|---|
| internal/specs/discovery.go::NormalizeGeminiDiscovery | 88 | 0 | 50/88 | 4/4 | 16/16 |
| internal/specs/fetcher.go::(*Fetcher).Get | 86 | 0 | 50/86 | 4/4 | 13/13 |
| internal/specs/openapi.go::NormalizeOpenAPI | 83 | 0 | 50/83 | 3/3 | 11/11 |
| internal/specs/openapi.go::(OpenAPINormalizer).Normalize | 80 | 0 | 50/80 | 3/3 | 10/10 |
| internal/specs/validator.go::(*Validator).Validate | 79 | 0 | 50/79 | 1/1 | 7/7 |
| internal/specs/validator.go::(*Validator).LoadRaw | 76 | 0 | 50/76 | 1/1 | 5/5 |
| internal/specs/discovery.go::LoadProviderSchema | 67 | 0 | 50/67 | 1/1 | 6/6 |
| internal/specs/loader.go::(PassThroughNormalizer).Normalize | 67 | 0 | 50/67 | 3/3 | 14/14 |
| internal/specs/loader.go::(*ContractLoader).Refresh | 54 | 0 | 50/54 | 3/3 | 11/11 |
| internal/specs/fetcher.go::NewFetcherWithFallback | 53 | 0 | 21/21 | 0/0 | 2/2 |
| internal/specs/validator.go::(*Validator).LoadNormalized | 52 | 0 | 50/52 | 1/1 | 6/6 |
| internal/specs/loader.go::(*ContractLoader).LoadFallback | 46 | 0 | 50/46 | 1/1 | 5/5 |
| internal/specs/registry.go::(Registry).List | 40 | 0 | 50/40 | 3/3 | 12/12 |
| internal/specs/registry.go::DefaultRegistry | 38 | 0 | 50/38 | 1/1 | 6/6 |
| internal/specs/validator.go::(*Validator).Has | 38 | 0 | 21/21 | 0/0 | 6/6 |
| internal/specs/registry.go::(Registry).Lookup | 37 | 0 | 50/37 | 1/1 | 8/8 |
| internal/specs/fetcher.go::NewFetcher | 33 | 0 | 21/21 | 0/0 | 3/3 |
| internal/specs/validator.go::(*Validator).Schemas | 32 | 0 | 21/21 | 1/1 | 8/8 |
| internal/specs/loader.go::NewContractLoader | 31 | 0 | 21/21 | 0/0 | 2/2 |
| internal/specs/validator.go::(*ValidationError).Error | 27 | 0 | 21/21 | 0/0 | 2/2 |
| internal/specs/vendored.go::VendoredFallbacks | 24 | 0 | 21/21 | 0/0 | 2/2 |
| internal/specs/registry.go::(ContractSource).HasRemote | 23 | 0 | 21/21 | 0/0 | 2/2 |
| internal/specs/registry.go::(ContractSource).Key | 23 | 0 | 21/21 | 0/0 | 2/2 |
| internal/specs/validator.go::NewValidator | 23 | 0 | 21/21 | 0/0 | 2/2 |

Sanity gate (flat == knob-absent, seed 1): PASS.
