# Plan: flt-it8.11 — Android Voice Capture App

## Context

Flotsam needs an Android app for quick voice note capture. The API already has:
- `POST /upload` — multipart file upload (auth required, max 50MB, supports `audio/mp4`/m4a)
- `captureVoiceNote` GraphQL mutation — takes `audioKey`, `title`, `tags`, `sensitivity`, `source`
- JWT auth (HS256) with `Authorization: Bearer <token>` header

The app should minimize friction: quick-action widget or notification shortcut for fast recording while driving.

## Implementation

### 1. Project scaffold (`android/`)

Standard Android/Gradle project with Kotlin DSL and version catalog:

```
android/
├── app/
│   ├── build.gradle.kts
│   └── src/main/
│       ├── AndroidManifest.xml
│       ├── java/com/flotsam/capture/
│       │   ├── MainActivity.kt          — single-activity host
│       │   ├── FlotsamApp.kt            — Application class
│       │   ├── ui/
│       │   │   ├── theme/Theme.kt       — Material 3 theme
│       │   │   ├── record/RecordScreen.kt   — main recording UI
│       │   │   ├── settings/SettingsScreen.kt — API URL + token config
│       │   │   └── navigation/Navigation.kt
│       │   ├── audio/AudioRecorder.kt   — MediaRecorder wrapper
│       │   ├── network/
│       │   │   ├── ApiClient.kt         — OkHttp + upload logic
│       │   │   └── GraphQLClient.kt     — captureVoiceNote mutation
│       │   ├── data/SettingsStore.kt    — DataStore preferences
│       │   └── widget/QuickCaptureReceiver.kt — notification shortcut
│       └── res/
│           ├── drawable/               — icons
│           ├── values/strings.xml
│           └── xml/shortcuts.xml       — app shortcuts
├── gradle/
│   ├── wrapper/
│   │   └── gradle-wrapper.properties
│   └── libs.versions.toml             — version catalog
├── build.gradle.kts                   — root build file
├── settings.gradle.kts
├── gradle.properties
├── gradlew / gradlew.bat
└── README.md
```

### 2. Dependencies (version catalog)

- **Jetpack Compose** BOM (latest stable) — UI
- **Material 3** — theming
- **OkHttp 4** — HTTP client for multipart upload
- **Gson** — JSON parsing (lightweight, no codegen needed)
- **DataStore Preferences** — settings persistence
- **Navigation Compose** — screen navigation
- **Compose Runtime** — for state management

### 3. Key components

**AudioRecorder** (`audio/AudioRecorder.kt`):
- Wraps `MediaRecorder` for M4A (AAC) recording
- Start/stop/cancel with temp file management
- Returns `File` path on completion

**ApiClient** (`network/ApiClient.kt`):
- `uploadAudio(file: File, token: String, baseUrl: String)` → `UploadResponse`
- Multipart POST to `/upload` with `file` field, `audio/mp4` content type
- JWT Bearer auth header

**GraphQLClient** (`network/GraphQLClient.kt`):
- `captureVoiceNote(audioKey: String, tags: List<String>, token: String, baseUrl: String)`
- Simple HTTP POST to `/graphql` with JSON body (no heavy GraphQL library needed)
- Source set to `"android_app"`

**RecordScreen** (`ui/record/RecordScreen.kt`):
- Large record button (Material 3 FAB style)
- Recording timer display
- Stop/cancel buttons during recording
- Optional tag input chip field
- Upload progress indicator
- Status feedback (success/error)

**SettingsScreen** (`ui/settings/SettingsScreen.kt`):
- API URL text field (default: empty, must configure)
- JWT token text field (paste from web app)
- Connection test button

**QuickCaptureReceiver** (`widget/QuickCaptureReceiver.kt`):
- Static app shortcut (long-press icon → "Quick Record")
- Notification shortcut for persistent quick-access
- Launches directly into recording mode

### 4. Permissions

```xml
<uses-permission android:name="android.permission.RECORD_AUDIO" />
<uses-permission android:name="android.permission.INTERNET" />
<uses-permission android:name="android.permission.POST_NOTIFICATIONS" />  <!-- Android 13+ -->
```

### 5. Upload flow

1. User taps record → `AudioRecorder.start()` → writes M4A to cache dir
2. User taps stop → `AudioRecorder.stop()` → returns `File`
3. App calls `ApiClient.uploadAudio(file)` → gets `UploadResponse{key, content_type, size}`
4. App calls `GraphQLClient.captureVoiceNote(audioKey=response.key, tags=userTags, source="android_app")`
5. Show success/error feedback
6. Clean up temp file

### 6. Files to create

All under `android/` in the repo root (~20 files total). No changes to existing Go or web code.

## Verification

- Gradle project structure follows Android conventions
- Kotlin source files have valid syntax (no compilation errors on inspection)
- `README.md` documents: prerequisites (Android Studio, JDK 17), build steps, configuration
- Manifest declares correct permissions and activities
- `.gitignore` excludes build artifacts, `.gradle/`, local properties
