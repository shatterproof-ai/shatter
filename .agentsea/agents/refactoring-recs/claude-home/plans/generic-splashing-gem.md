# flt-it8.32.17 — Android Static Analysis Rollout

## Context
Android code exists at `android/` with real Kotlin/Compose source (~14 `.kt` files), but **no tests exist yet**. The plan calls for Android lint and detekt integration, with thresholds deferred until meaningful tests land. The existing `scripts/ci/run-static-analyzers.sh` uses a graceful-skip pattern (check if tool exists, run, report).

## Deliverables

### 1. Add detekt config + Gradle integration
- **File**: `android/config/detekt/detekt.yml` — baseline config (default rules, no custom thresholds yet)
- **File**: `android/build.gradle.kts` — add detekt plugin
- **File**: `android/gradle/libs.versions.toml` — add detekt version

### 2. Add Android lint + detekt to `scripts/ci/run-static-analyzers.sh`
Same graceful-skip pattern as gosec/govulncheck/semgrep:
- **Android lint**: check for `android/gradlew`, run `./gradlew :app:lint`
- **detekt**: check for `android/gradlew`, run `./gradlew :app:detekt`
- Both skip gracefully if no Gradle wrapper or no Android SDK

### 3. Add `make android-lint` target to root Makefile
- Runs both Android lint and detekt via Gradle
- Gracefully skips if `android/gradlew` doesn't exist or `ANDROID_HOME` is unset

### 4. Wire into `lint` target
- Add `android-lint` as a dependency of the existing `lint` target (currently: `api-lint web-lint`)

## Files to modify
- `android/build.gradle.kts` — add detekt plugin
- `android/gradle/libs.versions.toml` — add detekt version + plugin
- `android/config/detekt/detekt.yml` — new file, baseline config
- `scripts/ci/run-static-analyzers.sh` — add Android lint + detekt sections
- `Makefile` — add `android-lint` target, update `lint` target

## Verification
```bash
# From worktree root:
make api-test-unit   # existing tests still pass
make api-lint        # existing lint still passes
# Verify android-lint skips gracefully (no Android SDK in CI):
make android-lint
```
