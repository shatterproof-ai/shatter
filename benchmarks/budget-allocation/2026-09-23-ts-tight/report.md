# Budget-allocation benchmark

Corpus: `/tmp/shatter-examples-main/standalone/ts`. Seeds: [1, 2, 3]. Per-function flat budget: 10 paths × 5 executions.

## Totals (median over seeds)

| arm | Σ branches covered / total | Σ lines covered / total | Σ executions used | Σ allocated | Σ claimed | wall s |
|---|---|---|---|---|---|---|
| flat | 109 / 235 | 262 / 1234 | 1185 | 0 | 0 | 15.6 |
| static | 112 / 235 | 263 / 1234 | 1279 | 1750 | 1 | 16.0 |

## Paired static − flat (per seed)

| metric | n | median Δ | per-seed |
|---|---|---|---|
| branches_covered | 3 | +1.0 | +0.0, +1.0, +7.0 |
| lines_covered | 3 | +0.0 | +0.0, +1.0, +0.0 |
| iterations | 3 | +100.0 | +67.0, +133.0, +100.0 |
| wall_s | 3 | +0.4 | +0.1, +0.4, +0.8 |

## Outcomes (seed 1)

| arm | behavioral | error_only |
|---|---|---|
| flat | 30 | 5 |
| static | 30 | 5 |

## Per function (seed 1, static allocation descending)

| function | allocated | claimed | executions flat/static | branches flat/static | lines flat/static |
|---|---|---|---|---|---|
| 10-path-router.ts::matchRoute | 111 | 0 | 21/21 | 1/1 | 2/2 |
| 14-semver.ts::satisfiesRange | 97 | 0 | 35/35 | 7/7 | 23/23 |
| 16-cron-parser.ts::parseCron | 84 | 0 | 50/84 | 3/3 | 8/8 |
| 18-accept-language.ts::negotiateLanguage | 84 | 0 | 23/23 | 1/1 | 3/3 |
| 07-auth-validation.ts::authorizeRequest | 81 | 0 | 29/29 | 3/3 | 6/6 |
| 06-nested-control-flow.ts::processStateMachine | 79 | 0 | 50/79 | 2/2 | 8/8 |
| 15-email-validator.ts::validateEmail | 74 | 0 | 50/74 | 13/13 | 13/13 |
| 05-unions.ts::routeRequest | 66 | 0 | 21/21 | 1/1 | 2/2 |
| 06-nested-control-flow.ts::classifyHttpResponse | 64 | 0 | 35/35 | 10/10 | 16/16 |
| 07-auth-validation.ts::validateJwt | 61 | 0 | 50/61 | 2/2 | 5/5 |
| 19-robots-policy.ts::evaluateRobotsPolicy | 61 | 0 | 50/61 | 2/2 | 3/3 |
| 20-dotenv-parser.ts::parseDotenv | 58 | 0 | 50/58 | 7/7 | 13/13 |
| 05-unions.ts::computeArea | 57 | 0 | 21/21 | 0/0 | 1/1 |
| 10-path-router.ts::resolveMiddleware | 52 | 0 | 29/29 | 6/6 | 12/12 |
| 09-rate-limiter.ts::checkRateLimit | 51 | 0 | 50/51 | 5/5 | 14/14 |
| 02-strings.ts::transformString | 43 | 0 | 29/29 | 5/5 | 10/10 |
| 14-semver.ts::parseSemver | 43 | 0 | 50/43 | 3/3 | 8/8 |
| 22-opaque-predicate.ts::classifyWithChecksum | 43 | 1 | 50/44 | 4/4 | 9/9 |
| 04-errors.ts::computeStats | 42 | 0 | 50/42 | 5/5 | 15/15 |
| 03-objects.ts::categorizeUser | 41 | 0 | 43/41 | 5/5 | 10/10 |
| 13-roman-numerals.ts::romanToInt | 40 | 0 | 50/40 | 3/3 | 10/10 |
| 02-strings.ts::classifyString | 38 | 0 | 43/38 | 5/5 | 10/10 |
| 17-mock-branches.ts::classifyConfigs | 38 | 0 | 22/22 | 1/1 | 5/5 |
| 03-objects.ts::describeRectangle | 35 | 0 | 32/32 | 3/3 | 8/8 |
| 17-mock-branches.ts::loadOrDefault | 35 | 0 | 21/21 | 1/1 | 2/2 |
| 17-mock-branches.ts::classifyStatus | 33 | 0 | 21/21 | 0/0 | 1/1 |
| 01-arithmetic.ts::classifyNumber | 29 | 0 | 28/28 | 3/3 | 7/7 |
| 01-arithmetic.ts::compareMagnitudes | 29 | 0 | 33/29 | 3/3 | 9/9 |
| 04-errors.ts::safeDivide | 29 | 0 | 33/29 | 3/3 | 7/7 |
| 13-roman-numerals.ts::intToRoman | 27 | 0 | 32/27 | 2/2 | 9/9 |
| 21-crypto-boundary.ts::classifySecret | 27 | 0 | 21/21 | 0/0 | 2/2 |
| 21-crypto-boundary.ts::encrypt | 26 | 0 | 21/21 | 0/0 | 2/2 |
| 13-mcdc-compound.ts::compoundAnd | 24 | 0 | 23/23 | 1/1 | 3/3 |
| 13-mcdc-compound.ts::compoundOr | 24 | 0 | 23/23 | 1/1 | 3/3 |
| 13-mcdc-compound.ts::threeWayAnd | 24 | 0 | 23/23 | 1/1 | 3/3 |

Sanity gate (flat == knob-absent, seed 1): PASS.
