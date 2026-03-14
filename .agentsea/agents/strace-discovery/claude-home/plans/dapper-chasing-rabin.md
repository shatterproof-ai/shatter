# flt-it8.26: Upload Content Sniffing

## Context

The upload endpoint (`api/internal/router/upload.go`) trusts the client-provided `Content-Type` header and filename extension to determine if a file is audio. An attacker can upload arbitrary files (executables, scripts, etc.) by spoofing the Content-Type header to `audio/mpeg` — the handler accepts it and stores it in S3. This is a P1 security bug.

## Approach

Add magic byte validation to the upload handler. After reading the multipart file, peek at the first 512 bytes and match against known audio format signatures. Reject uploads where the actual content doesn't match any supported audio format.

### Files to modify

- `api/internal/router/upload.go` — add `sniffAudioType()` function and integrate into handler
- `api/internal/router/upload_test.go` — comprehensive tests for content sniffing

### Implementation

#### 1. Add `sniffAudioType()` function in `upload.go`

A function that reads magic bytes and returns the detected MIME type (or empty string if not audio):

| Format | Magic bytes | Offset |
|--------|------------|--------|
| MP3 | `0xFF 0xFB`, `0xFF 0xF3`, `0xFF 0xF2` (frame sync) or `ID3` (ID3 tag) | 0 |
| WAV | `RIFF....WAVE` | 0 (RIFF at 0, WAVE at 8) |
| OGG | `OggS` | 0 |
| FLAC | `fLaC` | 0 |
| WebM | `0x1A 0x45 0xDF 0xA3` (EBML header, shared with Matroska) | 0 |
| M4A/MP4 | `ftyp` at offset 4 | 4 |

The function:
1. Accepts a `[]byte` (first 512 bytes of file)
2. Checks each signature in order
3. Returns detected MIME type or `""` if no match

#### 2. Modify `uploadHandler` to sniff content

After `r.FormFile("file")` succeeds:
1. Read the first 512 bytes into a buffer using `io.ReadAtLeast` (or just `io.ReadFull` with a smaller buffer)
2. Call `sniffAudioType(buf)` to detect actual format
3. If sniffed type is empty → reject with 400 "file content does not match a supported audio format"
4. Use the sniffed type as the authoritative content type (override the header)
5. Reconstruct the reader using `io.MultiReader(bytes.NewReader(buf), file)` so the full file streams to S3

Key design decision: **the sniffed type becomes the source of truth**. The client-provided Content-Type is ignored for type determination. This eliminates the entire class of mismatch attacks.

#### 3. Handle edge cases

- **Empty/truncated files**: If fewer than 12 bytes can be read, reject as invalid
- **WebM vs video Matroska**: Both share the EBML header. Accept both — if someone uploads a video with audio, the transcription worker will handle or reject it downstream. This is a pragmatic choice since WebM audio and video share the same container format.

#### 4. Tests in `upload_test.go`

| Test | Description |
|------|-------------|
| `TestSniffAudioType_ValidFormats` | Table-driven: valid magic bytes for each format → correct MIME type |
| `TestSniffAudioType_NonAudio` | PNG, JPEG, PDF, plain text, ELF binary headers → empty string |
| `TestSniffAudioType_TruncatedEmpty` | Empty slice, 1-byte slice → empty string |
| `TestSniffAudioType_MP3Variants` | ID3v2 tag, various frame sync bytes |
| `TestUploadHandler_ContentSniffReject` | Full handler test: multipart upload with `Content-Type: audio/mpeg` but PNG content → 400 |

For handler-level tests, `storage.Client` is a struct (not interface), so tests that need to reach the sniffing logic will need a non-nil storage. We can create a minimal mock by testing the `sniffAudioType` function directly (it's the core logic) and verifying handler integration through the rejection path (which returns before hitting storage).

## Verification

```bash
cd api && go test ./internal/router/ -run TestSniff -v   # sniff unit tests
make api-test-unit                                        # all unit tests pass
make api-lint                                             # linting passes
```
