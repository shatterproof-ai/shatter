# Budget-allocation benchmark

Corpus: `/tmp/shatter-examples-main/standalone/ts`. Seeds: [1, 2, 3]. Per-function flat budget: 100 paths × 5 executions.

## Totals (median over seeds)

| arm | Σ branches covered / total | Σ lines covered / total | Σ executions used | Σ allocated | Σ claimed | wall s |
|---|---|---|---|---|---|---|
| flat | 121 / 235 | 271 / 1234 | 3746 | 0 | 0 | 21.2 |
| static | 121 / 235 | 271 / 1234 | 4073 | 17500 | 0 | 21.7 |

## Paired static − flat (per seed)

| metric | n | median Δ | per-seed |
|---|---|---|---|
| branches_covered | 3 | +0.0 | +0.0, +0.0, +0.0 |
| lines_covered | 3 | +0.0 | +0.0, +0.0, +0.0 |
| iterations | 3 | +327.0 | +371.0, -28.0, +327.0 |
| wall_s | 3 | +0.5 | -0.1, +0.5, +0.9 |

## Outcomes (seed 1)

| arm | behavioral | error_only |
|---|---|---|
| flat | 30 | 5 |
| static | 30 | 5 |

## Per function (seed 1, static allocation descending)

| function | allocated | claimed | executions flat/static | branches flat/static | lines flat/static |
|---|---|---|---|---|---|
| 10-path-router.ts::matchRoute | 1472 | 0 | 21/21 | 1/1 | 2/2 |
| 14-semver.ts::satisfiesRange | 1247 | 0 | 35/35 | 7/7 | 23/23 |
| 18-accept-language.ts::negotiateLanguage | 1040 | 0 | 23/23 | 1/1 | 3/3 |
| 16-cron-parser.ts::parseCron | 1035 | 0 | 234/234 | 3/3 | 8/8 |
| 07-auth-validation.ts::authorizeRequest | 996 | 0 | 29/29 | 3/3 | 6/6 |
| 06-nested-control-flow.ts::processStateMachine | 964 | 0 | 233/233 | 2/2 | 8/8 |
| 15-email-validator.ts::validateEmail | 876 | 0 | 500/876 | 19/19 | 13/13 |
| 05-unions.ts::routeRequest | 750 | 0 | 21/21 | 1/1 | 2/2 |
| 06-nested-control-flow.ts::classifyHttpResponse | 723 | 0 | 35/35 | 10/10 | 16/16 |
| 07-auth-validation.ts::validateJwt | 674 | 0 | 232/232 | 2/2 | 5/5 |
| 19-robots-policy.ts::evaluateRobotsPolicy | 674 | 0 | 452/452 | 3/3 | 11/11 |
| 20-dotenv-parser.ts::parseDotenv | 625 | 0 | 500/625 | 7/7 | 21/21 |
| 05-unions.ts::computeArea | 609 | 0 | 21/21 | 0/0 | 1/1 |
| 10-path-router.ts::resolveMiddleware | 529 | 0 | 29/29 | 6/6 | 12/12 |
| 09-rate-limiter.ts::checkRateLimit | 509 | 0 | 261/261 | 5/5 | 14/14 |
| 02-strings.ts::transformString | 382 | 0 | 29/29 | 5/5 | 10/10 |
| 22-opaque-predicate.ts::classifyWithChecksum | 375 | 0 | 58/58 | 4/4 | 9/9 |
| 14-semver.ts::parseSemver | 373 | 0 | 236/236 | 3/3 | 8/8 |
| 04-errors.ts::computeStats | 370 | 0 | 500/370 | 5/5 | 15/15 |
| 03-objects.ts::categorizeUser | 342 | 0 | 43/43 | 5/5 | 10/10 |
| 13-roman-numerals.ts::romanToInt | 326 | 0 | 245/245 | 4/4 | 10/10 |
| 17-mock-branches.ts::classifyConfigs | 314 | 0 | 22/22 | 1/1 | 5/5 |
| 02-strings.ts::classifyString | 309 | 0 | 43/43 | 5/5 | 10/10 |
| 03-objects.ts::describeRectangle | 269 | 0 | 32/32 | 3/3 | 8/8 |
| 17-mock-branches.ts::loadOrDefault | 267 | 0 | 21/21 | 1/1 | 2/2 |
| 17-mock-branches.ts::classifyStatus | 238 | 0 | 21/21 | 0/0 | 1/1 |
| 01-arithmetic.ts::compareMagnitudes | 174 | 0 | 33/33 | 3/3 | 9/9 |
| 04-errors.ts::safeDivide | 171 | 0 | 33/33 | 3/3 | 7/7 |
| 01-arithmetic.ts::classifyNumber | 169 | 0 | 28/28 | 3/3 | 7/7 |
| 21-crypto-boundary.ts::classifySecret | 147 | 0 | 21/21 | 0/0 | 2/2 |
| 13-roman-numerals.ts::intToRoman | 145 | 0 | 32/32 | 2/2 | 9/9 |
| 21-crypto-boundary.ts::encrypt | 124 | 0 | 21/21 | 0/0 | 2/2 |
| 13-mcdc-compound.ts::compoundAnd | 94 | 0 | 23/23 | 1/1 | 3/3 |
| 13-mcdc-compound.ts::compoundOr | 94 | 0 | 23/23 | 1/1 | 3/3 |
| 13-mcdc-compound.ts::threeWayAnd | 94 | 0 | 23/23 | 1/1 | 3/3 |

Sanity gate (flat == knob-absent, seed 1): PASS.
