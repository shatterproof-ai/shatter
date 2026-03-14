# Plan: Audio Key Ownership Validation (flt-it8.25)

## Context

The `captureVoiceNote` resolver accepts any `audioKey` string from the client without verifying that the referenced S3 object belongs to the authenticated user. The transcription worker then downloads that key blindly. A malicious user could craft an `audioKey` pointing to another user's uploads, gaining access to their audio content via transcription.

The upload handler (`POST /upload`) generates keys with the pattern `{userID}/uploads/{uuid}.{ext}`, so ownership can be validated by checking the key prefix matches the authenticated user's ID.

## Changes

### 1. Add `ValidateKeyOwnership` to storage package

**File**: `api/internal/storage/keys.go`

Add a function that validates an S3 key belongs to a given user:

```go
func ValidateKeyOwnership(key string, ownerID uuid.UUID) error
```

Validation rules:
- Key must not be empty
- Key must start with `{ownerID}/` prefix
- Key must not contain path traversal sequences (`..`)
- Key must have at least 3 path segments (owner/type/rest)

Returns a descriptive error on failure, nil on success.

### 2. Add validation in the resolver (before item creation)

**File**: `api/graph/resolver/schema.resolvers.go` (line ~99, after `requireAuth`)

```go
if err := storage.ValidateKeyOwnership(input.AudioKey, claims.UserID); err != nil {
    return nil, fmt.Errorf("resolver: capture voice note: %w", err)
}
```

This prevents item creation with a foreign key.

### 3. Add validation in the worker (before S3 download)

**File**: `api/internal/worker/transcribe.go` (line ~56, before `w.Storage.Download`)

```go
if err := storage.ValidateKeyOwnership(job.Args.AudioKey, job.Args.OwnerID); err != nil {
    w.setError(ctx, job.Args.ItemID, job.Args.OwnerID, "audio key ownership validation failed")
    return fmt.Errorf("worker: transcribe: %w", err)
}
```

Defense-in-depth: even if a bad key gets into the job queue, the worker won't download it.

### 4. Write tests

**File**: `api/internal/storage/keys_test.go`

Test cases for `ValidateKeyOwnership`:
- Valid key with matching owner ID → no error
- Key with different owner ID → error
- Empty key → error
- Key with `..` path traversal → error
- Key with only owner prefix, no subpath → error
- Key with extra slashes / odd formatting → error

## Verification

```bash
cd /home/ketan/project/flotsam/.claude/worktrees/worktree/audio-key
make api-test-unit && make api-lint
```
