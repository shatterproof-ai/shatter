# flt-it8.30: Quality Gate Reliability

## Context
Current `go test ./... -short` passes, but several tests are placeholders (empty bodies, discarded variables) and the worker package at 46% coverage has significant gaps in security-critical paths. The transcribe worker's error propagation test is a stub. The issue asks us to make unit tests reliable by default and add substantive coverage for upload validation, auth flows, and worker security boundaries.

## Current State
- All tests pass with `go test ./... -short` — no reliability failures
- 1 placeholder test: `TestTranscribeWorker_TranscriberError` (transcribe_test.go:73-78) — creates mock but never executes anything
- Worker tests cover nil-dep guards but not actual work execution with mocks
- `TestWorkersReturnsNonNil` (worker_test.go:11-16) has a `testing.Short()` skip but empty body even for the non-short case
- Auth and upload tests are already substantive (89.7% and good coverage respectively)

## Plan

### 1. Fix placeholder test: `TestTranscribeWorker_TranscriberError`
**File:** `api/internal/worker/transcribe_test.go:73-78`

The transcribe worker's `Work()` requires `Storage` (for Download) and `Pool` (for SQL updates), which need DB/S3. But we can test the early-exit paths and error propagation by:
- Adding a `mockStorage` that returns a reader from `Download` (use `io.NopCloser(strings.NewReader("audio data"))`)
- The worker will then call `Transcriber.Transcribe()` which returns our mock error
- But then it calls `w.setError()` which needs `w.Pool` → will panic on nil Pool

**Approach:** Test that the transcriber error is propagated by providing mock storage but accepting that `setError` will fail silently (it logs but doesn't return). Actually, `setError` calls `w.Pool.Exec` which will panic on nil pool.

Better approach: Add a `mockReadCloser` for storage download, provide mock transcriber with error, and verify the worker returns an error. The `setError` call on line 75 will try to use `w.Pool` which is nil → panic.

**Revised approach:** We can't fully unit-test the transcribe worker's error path without a pool because `setError` does DB writes. Instead:
- Replace the placeholder with a test that documents this limitation clearly AND tests what we can: the nil-storage and nil-transcriber early exits are already tested
- Add a test for `TranscribeArgs` JSON serialization round-trip (verifying field mapping)
- Mark the full work-flow test as integration-only

### 2. Fix placeholder: `TestWorkersReturnsNonNil`
**File:** `api/internal/worker/worker_test.go:11-16`

This test skips in short mode and does nothing in long mode. Either:
- Remove it (it tests nothing)
- Or convert to an integration test that actually calls `NewRegistry`

**Action:** Remove or replace with a test that verifies `NewRegistry` returns a non-nil registry when given nil deps (which it does for unit testing).

### 3. Add substantive worker tests
**File:** `api/internal/worker/embed_test.go` — add test for successful embedding (mock embedder returns vector, but Pool is nil so DB write fails → verify error message)
**File:** `api/internal/worker/classify_test.go` — add test for successful classification (mock classifier returns result, Pool nil → verify error)

These test the actual work path through the mock providers, verifying error wrapping from the DB layer.

### 4. Add `TestPageFetchWorker_ErrorStatusOnFetchFailure`
**File:** `api/internal/worker/page_fetch_test.go`

Verify that when fetch fails, the item service receives an "error" status update. (This is partially covered by existing tests but let's make it explicit with error message assertions.)

### 5. Add upload handler integration test with multipart form
**File:** `api/internal/router/upload_test.go`

Add a test that constructs a real `multipart/form-data` request with a valid audio file and verifies the full handler path (auth check → content sniff → size check).

### 6. Verify and clean up Makefile test tiers
**File:** `api/Makefile` — already correct (`test-unit` uses `-short`, `test-integration` uses `-run Integration`)
**File:** `Makefile` (root) — verify `api-test-unit` calls the right target

No changes expected here.

## Files to Modify
1. `api/internal/worker/transcribe_test.go` — replace placeholder with substantive test or proper skip
2. `api/internal/worker/worker_test.go` — fix `TestWorkersReturnsNonNil` placeholder
3. `api/internal/worker/embed_test.go` — add DB-write error path test
4. `api/internal/worker/classify_test.go` — add DB-write error path test
5. `api/internal/worker/page_fetch_test.go` — add error message assertions

## Verification
```bash
cd api && go test -race -short ./...   # must pass with 0 failures
make api-test-unit && make api-lint    # quality gate
```
