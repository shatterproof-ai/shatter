# Web Voice Capture (flt-4z3)

## Context

The Capture page has Bookmark and Note tabs. The backend already supports voice notes: `POST /upload` accepts audio files (WebM, WAV, MP3, etc.) and `captureVoiceNote` GraphQL mutation creates the item + enqueues async transcription. We need to add browser-based recording UI to complete the end-to-end flow.

## Plan

### 1. Create `useAudioRecorder` hook
**New file:** `web/src/hooks/useAudioRecorder.ts`

Custom hook wrapping MediaRecorder API:
- States: `idle` → `recording` → `stopped` (with `paused` support)
- Returns: `{ status, audioBlob, duration, error, start, stop, pause, resume, reset }`
- `start()`: calls `getUserMedia({ audio: true })`, creates MediaRecorder (prefer `audio/webm`, fallback to browser default for Safari `audio/mp4`)
- `stop()`: assembles chunks into Blob, releases mic stream, clears interval
- Duration tracked via 1s interval timer
- Error handling: permission denied, no mic found
- Cleanup on unmount: stop stream + clear interval
- Uses `useRef` for MediaRecorder/chunks/stream/interval, `useState` for status/blob/duration/error

### 2. Create `uploadAudio` utility
**New file:** `web/src/lib/upload.ts`

```ts
export async function uploadAudio(blob: Blob): Promise<{ key: string; content_type: string; size: number }>
```
- Reads auth token via `useAuthStore.getState().getToken()` (same pattern as `urqlClient.ts`)
- Creates FormData, appends blob as `'file'` with filename `recording.webm`
- POSTs to `/upload` with Bearer token, returns parsed JSON
- Throws on non-ok response

### 3. Add `VoiceTab` to Capture page
**Modify:** `web/src/pages/Capture.tsx`

Add GraphQL mutation at module top:
```ts
const CaptureVoiceNoteMutation = graphql(`
  mutation CaptureVoiceNote($input: VoiceNoteInput!) {
    captureVoiceNote(input: $input) { id title type }
  }
`)
```

`VoiceTab` component (same pattern as BookmarkTab/NoteTab):
- **idle**: Record button (red ActionIcon with Mic icon) + "Tap to record" text
- **recording**: Pulsing red dot + mm:ss duration + Stop button (Square icon)
- **stopped**: `<audio controls>` playback preview + Re-record button + title/tags inputs + Save button
- Save flow: `uploadAudio(blob)` → `executeMutation({ input: { audioKey, title, tags, source: 'web' } })` → `navigate('/')`
- Loading state on save button, error alert same pattern as other tabs

Wire into Capture tabs:
```tsx
<Tabs.Tab value="voice" leftSection={<Mic size={16} />}>Voice</Tabs.Tab>
<Tabs.Panel value="voice" pt="md"><VoiceTab /></Tabs.Panel>
```

### 4. Recording pulse animation
**New file:** `web/src/pages/Capture.module.css`

Simple keyframe animation for the recording indicator dot. Imported as CSS module.

### 5. Tests

**New:** `web/src/hooks/useAudioRecorder.test.ts`
- Mock `navigator.mediaDevices.getUserMedia` and `MediaRecorder` via `vi.stubGlobal`
- Test state transitions: idle→recording→stopped, pause/resume, reset
- Test permission denied error
- Test duration increment with fake timers
- Use `renderHook` + `act`

**New:** `web/src/lib/upload.test.ts`
- Mock `global.fetch` and `useAuthStore`
- Test successful upload returns parsed JSON
- Test auth header included
- Test error thrown on non-ok response

**Modify:** `web/src/pages/Capture.test.tsx`
- Add test: Voice tab rendered alongside Bookmark and Note
- Add test: clicking Voice tab shows recording UI

## File inventory

| File | Action |
|---|---|
| `web/src/hooks/useAudioRecorder.ts` | Create |
| `web/src/hooks/useAudioRecorder.test.ts` | Create |
| `web/src/lib/upload.ts` | Create |
| `web/src/lib/upload.test.ts` | Create |
| `web/src/pages/Capture.tsx` | Modify |
| `web/src/pages/Capture.module.css` | Create |
| `web/src/pages/Capture.test.tsx` | Modify |

## Key references
- Upload pattern: `web/src/lib/urqlClient.ts` (auth token access)
- Tab pattern: `web/src/pages/Capture.tsx` (BookmarkTab/NoteTab)
- Test pattern: `web/src/pages/Capture.test.tsx`, `web/src/test/mocks.ts`
- Backend schema: `web/schema.graphql` (VoiceNoteInput already synced)

## Verification
```bash
cd web && pnpm build && pnpm lint && pnpm test
```
All three must pass with zero errors/warnings.
