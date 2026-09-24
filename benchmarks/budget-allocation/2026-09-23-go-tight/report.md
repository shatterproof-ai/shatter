# Budget-allocation benchmark

Corpus: `/tmp/shatter-examples-main/standalone/go`. Seeds: [1, 2, 3]. Per-function flat budget: 10 paths × 5 executions.

## Totals (median over seeds)

| arm | Σ branches covered / total | Σ lines covered / total | Σ executions used | Σ allocated | Σ claimed | wall s |
|---|---|---|---|---|---|---|
| flat | 103 / 250 | 253 / 1171 | 1187 | 0 | 0 | 39.4 |
| static | 117 / 250 | 278 / 1171 | 1256 | 1500 | 0 | 40.2 |

## Paired static − flat (per seed)

| metric | n | median Δ | per-seed |
|---|---|---|---|
| branches_covered | 3 | +16.0 | +18.0, +14.0, +16.0 |
| lines_covered | 3 | +25.0 | +25.0, +0.0, +25.0 |
| iterations | 3 | +69.0 | +69.0, +69.0, +92.0 |
| wall_s | 3 | +0.8 | +0.3, +2.7, +0.8 |

## Per seed (static vs flat)

| seed | Δ branches | Δ lines | Δ wall s | claimed (static) | functions lost | functions gained | Σ allocated == n×budget |
|---|---|---|---|---|---|---|---|
| 1 | +18 | +25 | +0.3 | 0 | 0 | 1 (15-email-validator.go::ValidateEmail) | yes |
| 2 | +14 | +0 | +2.7 | 0 | 0 | 1 (15-email-validator.go::ValidateEmail) | yes |
| 3 | +16 | +25 | +0.8 | 23 | 1 (13-roman-numerals.go::RomanToInt) | 1 (15-email-validator.go::ValidateEmail) | yes |

## Outcomes (seed 1)

| arm | behavioral | error_only |
|---|---|---|
| flat | 27 | 3 |
| static | 27 | 3 |

## Per function (seed 1, static allocation descending)

| function | allocated | claimed | executions flat/static | branches flat/static | lines flat/static |
|---|---|---|---|---|---|
| 10-path-router.go::MatchRoute | 100 | 0 | 21/21 | 1/1 | 3/3 |
| 15-email-validator.go::ValidateEmail | 93 | 0 | 50/93 | 2/20 | 6/31 |
| 18-accept-language.go::NegotiateLanguage | 84 | 0 | 21/21 | 1/1 | 3/3 |
| 07-auth-validation.go::AuthorizeRequest | 78 | 0 | 21/21 | 1/1 | 3/3 |
| 19-robots-policy.go::EvaluateRobotsPolicy | 76 | 0 | 50/76 | 3/3 | 9/9 |
| 16-cron-parser.go::ParseCron | 75 | 0 | 50/75 | 3/3 | 7/7 |
| 20-dotenv-parser.go::ParseDotenv | 70 | 0 | 50/70 | 11/11 | 18/18 |
| 08-data-transform.go::MergeConfig | 62 | 0 | 50/62 | 3/3 | 7/7 |
| 06-nested-control-flow.go::ClassifyHttpResponse | 61 | 0 | 35/35 | 10/10 | 17/17 |
| 06-nested-control-flow.go::ProcessStateMachine | 57 | 0 | 50/57 | 2/2 | 8/8 |
| 10-path-router.go::ResolveMiddleware | 55 | 0 | 50/55 | 8/8 | 14/14 |
| 05-unions.go::RouteRequest | 54 | 0 | 50/54 | 2/2 | 5/5 |
| 08-data-transform.go::TransformRecord | 54 | 0 | 50/54 | 1/1 | 4/4 |
| 14-semver.go::ParseSemver | 52 | 0 | 50/52 | 3/3 | 8/8 |
| 09-rate-limiter.go::CheckRateLimit | 51 | 0 | 50/51 | 6/6 | 16/16 |
| 14-semver.go::SatisfiesRange | 50 | 0 | 50/50 | 11/11 | 28/28 |
| 02-strings.go::TransformString | 44 | 0 | 39/39 | 4/4 | 10/10 |
| 13-roman-numerals.go::RomanToInt | 40 | 0 | 50/40 | 3/3 | 10/10 |
| 03-objects.go::CategorizeUser | 39 | 0 | 43/39 | 5/5 | 11/11 |
| 02-strings.go::ClassifyString | 35 | 0 | 24/24 | 4/4 | 8/8 |
| 03-objects.go::DescribeRectangle | 35 | 0 | 50/35 | 3/3 | 8/8 |
| 04-errors.go::ClassifyAge | 34 | 0 | 40/34 | 5/5 | 11/11 |
| 13-roman-numerals.go::IntToRoman | 30 | 0 | 50/30 | 2/2 | 7/7 |
| 01-arithmetic.go::ClassifyNumber | 28 | 0 | 31/28 | 3/3 | 8/8 |
| 01-arithmetic.go::CompareMagnitudes | 28 | 0 | 50/28 | 3/3 | 8/8 |
| 04-errors.go::SafeDivide | 28 | 0 | 27/27 | 2/2 | 6/6 |
| smoke-main.go::SmokeClassify | 24 | 0 | 22/22 | 1/1 | 4/4 |
| 05-unions.go::(Circle).Kind | 21 | 0 | 21/21 | 0/0 | 2/2 |
| 05-unions.go::(RectangleShape).Kind | 21 | 0 | 21/21 | 0/0 | 2/2 |
| 05-unions.go::(Triangle).Kind | 21 | 0 | 21/21 | 0/0 | 2/2 |

Sanity gate (flat == knob-absent, seed 1): PASS.
