---
name: walkthrough-review
description: Run the demo walkthrough and evaluate its output for human readability. Use after changing CLI output, explore formatting, log levels, or protocol logging.
user-invocable: true
---

## Purpose

Evaluate whether `demo/walkthrough.sh` output is useful and readable for a human user. Run this after any change that affects what shatter prints — explore reports, log formatting, verbosity flags, cluster summaries, etc.

## What a human wants from this output

A developer running shatter is asking: **"What does this function actually do, and are there any surprises?"**

The output should answer these questions, in priority order:

### 1. How many distinct behaviors does this function have?

The path count is the headline. A function with 2 paths is simple; one with 12 might be hiding complexity. This number should be immediately visible.

### 2. What input conditions trigger each behavior?

Constraints on the function's actual parameter names, in declaration order. Express constraints with mathematical precision and conciseness — `b = 0`, `n < 0`, `s.length > 10 && s[0] = "/"` — not natural language paraphrases and not raw solver output.

### 3. What is the outcome of each behavior?

For each path, one of:
- **Return value**: the concrete value or a description of the return expression
- **Thrown error**: the error/exception type in languages with typed errors (e.g., `Error`, `TypeError`, `ValueError`, `std::io::Error`), or the error value in languages where errors are scalars (e.g., Go's `error` string, Rust's enum variant). Include the error message if it's a fixed string.
- **Side effects**: if the path's primary observable effect is a side effect rather than a return

### 4. Concrete examples per path

2-3 representative input/output pairs per path cluster. Arguments shown with their parameter names in declaration order. These ground the abstract constraints in specific values a developer can paste into a REPL.

### 5. Are there surprises?

Paths the developer likely didn't anticipate should stand out: unhandled edge cases, implicit type coercions, unexpected nulls/NaN, unreachable code that turned out to be reachable. These are the primary value of exploratory testing.

### 6. How much of the input space does each path cover?

Is the error case a narrow edge (measure-zero like `b = 0`) or half the domain (`n < 0`)? This tells the developer whether a path is a corner case or a primary behavior.

### 7. Exploration completeness

Did exploration exhaust the path space, or hit iteration/timeout limits with unexplored branches remaining? The human needs to know if the map is complete or partial.

### 8. Batch triage (multi-function runs)

When exploring multiple functions, a one-line-per-function summary lets the developer scan and triage: which functions are simple (2 paths, no errors) and which are complex or problematic.

## Procedure

### 1. Run the walkthrough and capture output

```bash
./demo/walkthrough.sh --auto --delay 0 2>&1 | tee /tmp/shatter-walkthrough-review.txt
```

If the walkthrough fails to run (build errors, missing examples), report that and stop — fixing the build is a prerequisite.

### 2. Read the captured output and evaluate against each criterion

Go through the criteria below one by one. For each, quote the specific lines that pass or fail.

### 3. Report findings and suggest fixes

For each failing criterion:
- Quote the offending output (first few lines, not the whole block)
- Explain what's wrong
- Suggest a specific fix: which file, which function, what change

## Evaluation Criteria

### A. No protocol noise at INFO level

**Fail** if the output at default verbosity contains:
- Raw JSON objects (`{"protocol_version":...}`, `{"id":...,"command":...}`)
- Lines prefixed with `[shatter-ts]`, `[shatter-go]`, or `[shatter-rust]`
- Full stack traces (multi-line `at Function...` / `at Object...` blocks)

These belong at TRACE level only.

### B. Every function has a summary header

For each explored function, the output must include:
- The function name
- Its source file and line number
- The number of distinct paths discovered

**Fail** if a function's results appear without this context, or if the only output is a bare "Exploration complete: N iterations" line with no per-function breakdown.

### C. Path conditions use parameter names and mathematical notation

Each path should be described by constraints on the function's actual parameter names, in declaration order. Constraints should be mathematically precise and concise.

**Fail** if:
- Constraints use generic names like `arg0`, `arg1` instead of actual parameter names
- Constraints are verbose natural language ("when the second argument is equal to zero")
- Constraints are raw solver output (`(= (_ bv0 64) arg1)`)
- Parameters appear in an order different from the function signature

### D. Error paths describe the error type or value

When a path throws or returns an error:
- In typed-error languages (TypeScript, Java, Python, Rust): show the error/exception type (`Error`, `TypeError`, `DivisionByZero`)
- In scalar-error languages (Go): show the error string or value
- Include the error message if it's a fixed string

**Fail** if errors are described only as "throws an error" or "returns an error" without the type/value.

### E. Examples are concrete and selective

Each path cluster should show 2-3 concrete input/output examples with parameter names in declaration order.

**Fail** if:
- A cluster shows no examples
- A cluster dumps every execution (10+ examples)
- Examples lack actual values (showing placeholders or types instead)
- Parameter names are missing from examples

### F. Output volume is proportional to function count, not iteration count

A function explored with 100 iterations should produce roughly the same output as one explored with 20 (assuming similar path count). Iteration count is an engine detail.

**Fail** if total output lines scale with iteration count rather than the number of functions and paths. Guideline: fewer than 20 lines per function at INFO level.

### G. Performance data only when requested

Performance stats (timing, memory, iteration counts) should appear only when `--timing` is passed.

**Fail** if performance data appears in default output. If `--timing` is not yet implemented, note as "not yet applicable."

### H. Console-appropriate formatting

**Fail** if the output contains:
- Timestamps on every line
- `[INFO]` / `[DEBUG]` / `[WARN]` prefixes at the default log level
- Machine-readable structured output where human-readable text is expected

### I. Batch triage summary

When multiple functions are explored in one invocation, the output should include a scannable summary — one line per function with path count and whether errors were found.

**Fail** if the only way to assess multiple functions is to read through the full detail of each one sequentially.

### J. Completeness signal

The output should indicate whether exploration is complete (all paths found) or partial (hit limits).

**Fail** if there's no way to distinguish "3 paths, fully explored" from "3 paths found before hitting the iteration cap."

### K. No errors, crashes, or skipped functions due to bugs

The walkthrough is also a correctness gate. Check the ERROR SUMMARY at the end of the output, and scan for:

- `[error]` lines in stderr (exploration errors, deserialization failures, frontend crashes)
- `Command exited with status N` (unexpected non-zero exit codes)
- Functions skipped due to errors (e.g., "skipped: error: exploration error: ...")
- `failed to deserialize`, `panic`, `SIGSEGV`, or similar crash indicators

**Expected failures** (do NOT count as errors):
- `stale` command exit code 1 (means "some functions are stale" — informational, not a failure)
- Scan errors for `11-opaque-types.ts` and `12-external-deps.ts` (opaque types, missing external modules)

**Fail** if any step produces unexpected errors, crashes, or skips functions that should succeed. File a beads issue for each distinct failure.

## Output Format

Structure your report as:

```
## Walkthrough Review

**Overall**: PASS / MIXED / FAIL

### A. No protocol noise at INFO level — PASS/FAIL
[details]

### B. Every function has a summary header — PASS/FAIL
[details]

...

## Suggested Fixes
1. [file:line] — [what to change and why]
2. ...
```
