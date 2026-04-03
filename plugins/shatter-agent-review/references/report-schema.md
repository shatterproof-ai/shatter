# Review And Report Schema

Use these schemas to keep the downstream review and issue report consistent.

## Review output

Use these headings exactly:

### Overall interpretation

- one short paragraph describing what the target appears to do
- whether the run looks complete, partial, or failed

### Most important cases

List 3-7 cases in priority order. For each case include:

- `Case`: short label
- `Why it matters`: plain-language impact
- `Human summary`: what this behavior means
- `Representative sample`: one concrete input and output or error

### Precise observed results

For each important case, preserve the exact evidence that supports it:

- concrete inputs
- exact output or thrown error
- path condition, constraint, or spec fragment when available
- source artifact reference

### Possible issues or ambiguities

List only items that may require follow-up. Separate:

- likely Shatter issue
- likely target-program issue
- incomplete or ambiguous result

### Recommended next step

State the single next best action: rerun with spec output, inspect one target manually, or write an issue report.

## Markdown issue report

Use these headings exactly:

## Run summary

- command
- target
- run directory
- Shatter version
- completion status

## Environment and project context

Insert the captured output from `collect-context.sh`.

## Enumerated issues

Create one numbered subsection per issue:

### Issue 1: <title>

- `Severity`: critical, high, medium, or low
- `Category`: tool bug, usability, incomplete exploration, ambiguity
- `Why it matters`: impact on trust, correctness, or workflow
- `Human description`: plain-language summary
- `Reproduction`: exact command and relevant setup
- `Expected behavior`: what the user expected
- `Actual behavior`: what happened
- `Precise evidence`: exact outputs, samples, or spec fragments
- `Artifacts`: relevant file paths

If there are no issues, replace this section with `No actionable issues found.`

## Evidence and artifact references

List the files that back the report:

- stdout capture
- stderr capture
- spec or report outputs
- context capture
