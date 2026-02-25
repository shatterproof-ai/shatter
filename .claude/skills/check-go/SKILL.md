---
name: check-go
description: Run Go quality gates (go test + go vet). Use after making changes to shatter-go/.
allowed-tools: Bash
disable-model-invocation: true
---

Run the following checks and examine the output for errors:

1. Run `go test ./...` in `shatter-go/` and capture output
2. Run `go vet ./...` in `shatter-go/` and capture output
3. Examine both outputs for failures, vet warnings, and errors
4. Summarize results:
   - Number of tests passed/failed
   - Any vet warnings or errors
   - Suggested corrections for any failures
   - Overall PASS/FAIL status
