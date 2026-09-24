# Budget-allocation benchmark

Corpus: `/tmp/shatter-examples-main/standalone/go`. Seeds: [1, 2, 3]. Per-function flat budget: 100 paths × 5 executions.

## Totals (median over seeds)

| arm | Σ branches covered / total | Σ lines covered / total | Σ executions used | Σ allocated | Σ claimed | wall s |
|---|---|---|---|---|---|---|
| flat | 132 / 250 | 297 / 1171 | 5475 | 0 | 0 | 53.3 |
| static | 132 / 250 | 301 / 1171 | 6195 | 15000 | 0 | 54.2 |

## Paired static − flat (per seed)

| metric | n | median Δ | per-seed |
|---|---|---|---|
| branches_covered | 3 | +0.0 | +0.0, +0.0, +0.0 |
| lines_covered | 3 | +0.0 | -1.0, +23.0, +0.0 |
| iterations | 3 | +720.0 | +717.0, +720.0, +723.0 |
| wall_s | 3 | +0.9 | +0.9, +0.8, +1.4 |

## Outcomes (seed 1)

| arm | behavioral | error_only |
|---|---|---|
| flat | 27 | 3 |
| static | 27 | 3 |

## Per function (seed 1, static allocation descending)

| function | allocated | claimed | executions flat/static | branches flat/static | lines flat/static |
|---|---|---|---|---|---|
| 10-path-router.go::MatchRoute | 1298 | 0 | 21/21 | 1/1 | 3/3 |
| 15-email-validator.go::ValidateEmail | 1185 | 0 | 500/1185 | 22/22 | 42/42 |
| 18-accept-language.go::NegotiateLanguage | 1032 | 0 | 21/21 | 1/1 | 3/3 |
| 07-auth-validation.go::AuthorizeRequest | 941 | 0 | 21/21 | 1/1 | 3/3 |
| 19-robots-policy.go::EvaluateRobotsPolicy | 903 | 0 | 474/474 | 6/6 | 24/24 |
| 16-cron-parser.go::ParseCron | 897 | 0 | 232/232 | 3/3 | 7/7 |
| 20-dotenv-parser.go::ParseDotenv | 817 | 0 | 500/817 | 11/11 | 18/18 |
| 08-data-transform.go::MergeConfig | 686 | 0 | 257/257 | 3/3 | 7/7 |
| 06-nested-control-flow.go::ClassifyHttpResponse | 668 | 0 | 35/35 | 10/10 | 17/17 |
| 06-nested-control-flow.go::ProcessStateMachine | 612 | 0 | 232/232 | 2/2 | 8/8 |
| 10-path-router.go::ResolveMiddleware | 565 | 0 | 267/267 | 8/8 | 16/16 |
| 08-data-transform.go::TransformRecord | 561 | 0 | 231/231 | 1/1 | 4/4 |
| 05-unions.go::RouteRequest | 552 | 0 | 233/233 | 2/2 | 5/5 |
| 14-semver.go::ParseSemver | 517 | 0 | 237/237 | 3/3 | 8/8 |
| 09-rate-limiter.go::CheckRateLimit | 513 | 0 | 266/266 | 6/6 | 16/16 |
| 14-semver.go::SatisfiesRange | 500 | 0 | 393/393 | 12/12 | 31/31 |
| 02-strings.go::TransformString | 390 | 0 | 39/39 | 4/4 | 10/10 |
| 13-roman-numerals.go::RomanToInt | 354 | 0 | 500/354 | 5/5 | 15/15 |
| 03-objects.go::CategorizeUser | 335 | 0 | 43/43 | 5/5 | 11/11 |
| 02-strings.go::ClassifyString | 273 | 0 | 24/24 | 4/4 | 8/8 |
| 03-objects.go::DescribeRectangle | 270 | 0 | 236/236 | 3/3 | 8/8 |
| 04-errors.go::ClassifyAge | 248 | 0 | 40/40 | 5/5 | 11/11 |
| 13-roman-numerals.go::IntToRoman | 189 | 0 | 232/189 | 2/2 | 7/7 |
| 01-arithmetic.go::CompareMagnitudes | 157 | 0 | 253/157 | 3/3 | 9/8 |
| 01-arithmetic.go::ClassifyNumber | 153 | 0 | 31/31 | 3/3 | 8/8 |
| 04-errors.go::SafeDivide | 148 | 0 | 27/27 | 2/2 | 6/6 |
| smoke-main.go::SmokeClassify | 86 | 0 | 22/22 | 1/1 | 4/4 |
| 05-unions.go::(Circle).Kind | 50 | 0 | 21/21 | 0/0 | 2/2 |
| 05-unions.go::(RectangleShape).Kind | 50 | 0 | 21/21 | 0/0 | 2/2 |
| 05-unions.go::(Triangle).Kind | 50 | 0 | 21/21 | 0/0 | 2/2 |

Sanity gate (flat == knob-absent, seed 1): PASS.
