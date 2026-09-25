# Shatter Scan Report

- **Functions discovered:** 12
- **Functions attempted:** 5
- **Functions completed:** 1
  - **with observed behavior:** 1
  - **error-only (all discovered inputs threw):** 0
- **Functions failed:** 4
- **Functions skipped:** 7
  - **interrupted (total scan budget exceeded):** 7

## Source Set Summary

| Bucket | Files | Lines |
|--------|-------|-------|
| `production_ish` | 12 | 630 |
| `test_spec` | 0 | 0 |
| `generated` | 0 | 0 |
| `declaration_only` | 0 | 0 |
| `fixture_sample` | 0 | 0 |
| `policy_excluded` | 0 | 0 |
| `unsupported` | 0 | 0 |

- **Production-ish source lines:** 630 (whole-source denominator; see Coverage section below)

## Coverage

- **Attempted-function span:** 5 of 12 discovered functions attempted
- **Total branches:** 3 (across completed functions)
- **Branches covered:** 3
- **Overall coverage (completed-functions subset):** 100.0%

## Low-Coverage Buckets

*Completing a function (running it without a crash) is not the same as achieving high source or branch coverage. These buckets classify why coverage stays shallow so the low numbers can be acted on directly.*

Low-coverage cutoff: completed functions with coverage ≤ 10%.

| Bucket | Count |
|--------|-------|
| Failed (attempted, did not complete) | 4 |
| Skipped / unsupported | 0 |
| Zero coverage with branches (needs setup/mocks) | 0 |
| Zero coverage, no branches (accessors/getters) | 0 |
| Positive but low coverage | 0 |

## Function Summary

| Status | Outcome | Function | File | Coverage | Branches | Lines | Iterations |
|--------|---------|----------|------|----------|----------|-------|------------|
| PASS | behavioral | `classifyNumber` | /tmp/claude-1000/-home-ketan-project-shatter/63ab6471-d62f-4111-8969-df8076934da8/scratchpad/proj/mix/01-arithmetic-copy.ts | 100.0% | 3/3 | 7/7 | 30 |

## Function Details

### `classifyNumber`

- **File:** /tmp/claude-1000/-home-ketan-project-shatter/63ab6471-d62f-4111-8969-df8076934da8/scratchpad/proj/mix/01-arithmetic-copy.ts
- **Coverage:** 100.0%
- **Branches:** 3/3
- **Lines:** 7/7
- **Iterations:** 30
- **Constraints collected:** 60

**Behaviors:**

- Cluster 0: returns "zero" (inputs: 0)
- Cluster 1: returns "negative" (inputs: -1)
- Cluster 2: returns "positive-even" (inputs: 2)
- Cluster 3: returns "positive-odd" (inputs: 1)

## Interesting Inputs

### `classifyNumber`

- 0 -> "zero"
- -1 -> "negative"

## Failed Functions

- `ClassifyNumber` (/tmp/claude-1000/-home-ketan-project-shatter/63ab6471-d62f-4111-8969-df8076934da8/scratchpad/proj/mix/01-arithmetic.go): timed out during build after 120s
- `CompareMagnitudes` (/tmp/claude-1000/-home-ketan-project-shatter/63ab6471-d62f-4111-8969-df8076934da8/scratchpad/proj/mix/01-arithmetic.go): timed out during task after 125s
- `ClassifyAge` (/tmp/claude-1000/-home-ketan-project-shatter/63ab6471-d62f-4111-8969-df8076934da8/scratchpad/proj/mix/04-errors.go): timed out during task after 125s
- `SafeDivide` (/tmp/claude-1000/-home-ketan-project-shatter/63ab6471-d62f-4111-8969-df8076934da8/scratchpad/proj/mix/04-errors.go): timed out during task after 125s

## Skipped (Interrupted)

7 function(s) not attempted (total budget 300s exhausted).
Raise --timeout-total or narrow the target to reach the remaining functions.

