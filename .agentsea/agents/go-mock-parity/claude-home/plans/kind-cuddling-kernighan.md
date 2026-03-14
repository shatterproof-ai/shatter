# Plan: Android CI Readiness (flt-2gt)

## Context

The Android app (`android/`) is a real Kotlin/Compose project with source code, tests, and build configs, but it's missing the Gradle wrapper executables (`gradlew`, `gradlew.bat`, `gradle-wrapper.jar`). This means:
- `make android-lint` silently skips
- `run-static-analyzers.sh` Android/detekt sections silently skip
- `run-changed.sh` doesn't detect `android/*` changes at all
- CI workflows have no Android steps

The app is presented as a shipped surface (referenced in README) but has zero CI coverage.

## Changes

### 1. Add Gradle wrapper files
- Run `gradle wrapper --gradle-version 8.11.1` inside `android/` to generate `gradlew`, `gradlew.bat`, and `gradle/wrapper/gradle-wrapper.jar`
- The `gradle-wrapper.properties` already exists pointing to 8.11.1
- These files must be committed to the repo (standard practice)

### 2. Add `android/*` detection to `run-changed.sh`
- Add `android_changed=0` variable (line ~53)
- Add `android/*) android_changed=1 ;;` case in the detection loop (line ~59)
- Add Android checks block: run `make android-lint` when `android_changed=1` (advisory, not strict)

**File:** `scripts/ci/run-changed.sh`

### 3. Wire `android-lint` into the `lint` Makefile target
- Change `lint: api-lint web-lint` → `lint: api-lint web-lint android-lint`
- Since `android-lint` already gracefully skips when gradlew is missing, this is safe

### 4. Add `android-test` Makefile target
- New target that runs `./gradlew :app:test` (JVM unit tests only — no emulator needed)
- Same graceful-skip pattern as `android-lint`

**File:** `Makefile`

### 5. Add Android CI job to `.github/workflows/ci.yml`
- New `android` job that runs on `ubuntu-latest`
- Uses `actions/setup-java@v4` with JDK 17
- Caches Gradle home (`~/.gradle/caches`, `~/.gradle/wrapper`)
- Runs: `make android-lint` and `make android-test`
- Runs after `standard` (parallel with `coverage` and `analyzers`) to avoid slowing the critical path
- No Android SDK setup needed — AGP downloads what it needs via `sdkmanager`

### 6. Update docs
- Add `make android-lint` and `make android-test` to the CLAUDE.md commands table
- Add Android prerequisites note to CLAUDE.md (JDK 17 for command-line builds)
- Verify `android/README.md` still matches (it references `./gradlew` which will now exist)

## Files to modify
- `android/gradlew` (new — generated)
- `android/gradlew.bat` (new — generated)
- `android/gradle/wrapper/gradle-wrapper.jar` (new — generated)
- `Makefile` — add `android-test`, wire `android-lint` into `lint`
- `scripts/ci/run-changed.sh` — add `android/*` detection
- `.github/workflows/ci.yml` — add `android` job
- `CLAUDE.md` — add Android targets to commands table

## Verification
1. Run `android/gradlew --version` to confirm wrapper works
2. Run `make android-lint` — should invoke Gradle (may fail without Android SDK locally, but should not silently skip)
3. Run `make android-test` — should invoke Gradle test task
4. Verify `run-changed.sh` detects android file changes
5. Review CI YAML for syntax correctness
