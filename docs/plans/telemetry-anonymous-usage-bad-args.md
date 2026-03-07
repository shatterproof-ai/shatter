# Shatter Telemetry: Anonymous Usage & Bad-Args Reporting

## Context

Shatter is distributed as a standalone binary to engineers and their agents. When users try CLI flags or subcommands that don't exist, that signal is lost — we don't know what features users expect. Additionally, we lack basic usage metrics (which commands are popular, success rates, common configurations). This feature adds a privacy-preserving, offline-capable telemetry system that queues events locally and (in a future phase) transmits them to an analytics backend.

## Scope

**This plan:** Create beads issues for the telemetry feature. No implementation code — only issue creation with detailed design captured in descriptions/notes.

**Phase 1 (issues):** Client-side telemetry module — event capture, consent management, local JSONL queue, `shatter telemetry` CLI subcommand. No network transmission.

**Phase 2 (blocked issue):** PostHog integration for server-side analytics. See "Backend Selection" section below.

---

## Backend Selection: PostHog vs Alternatives

| Solution | License | Hosting | Rust SDK | Strengths | Weaknesses | Fit |
|---|---|---|---|---|---|---|
| **PostHog** | MIT (self-host) or Cloud | Self-host or managed | Community crate `posthog-rs` | Full analytics suite (funnels, dashboards, retention, feature flags), open-source self-hostable, generous free tier (1M events/mo), built for product analytics | Heavier than needed for just event ingestion, community Rust SDK not official | **Best fit** — dashboards + funnels out of the box, no custom server to build |
| **Segment** | Proprietary SaaS | Cloud only | `segment` crate | Industry standard CDP, routes to 300+ downstream tools, official-quality Rust crate | Proprietary, paid after free tier, vendor lock-in, overkill for a single-product CLI | Overpowered |
| **Amplitude** | Proprietary SaaS | Cloud only | No Rust SDK (HTTP API) | Strong product analytics, good free tier | No Rust SDK, proprietary, focused on web/mobile not CLI tools | Poor fit |
| **Custom server** | N/A | Self-host | N/A | Full control, minimal dependencies | Must build dashboards, ingestion, storage, retention — significant ongoing work | Only if privacy requirements preclude any SaaS |
| **telemetry-kit** | MIT/Apache-2.0 | Self-host (Postgres+Redis) | Native Rust | Purpose-built for CLI tools, privacy-first, consent UI built in | Requires self-hosting Postgres+Redis, small community, less mature | Good but operational overhead |

**Recommendation:** PostHog (cloud free tier to start, self-host option if needed later). The `posthog-rs` crate provides `capture()` with properties, which maps directly to our event schema. PostHog's built-in dashboards eliminate the need to build any analytics UI.

---

## Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Consent model | Opt-out with first-run notice | Maximizes signal for bad-args use case; respects `DO_NOT_TRACK` standard |
| Storage location | `~/.local/share/shatter/telemetry/` (XDG) | User-level, not project-level; telemetry is about the tool not the project |
| Config location | `~/.config/shatter/telemetry.yaml` (XDG) | Separate from project `.shatter/config.yaml` |
| Queue format | JSONL (one event per line) | Simple, appendable, readable, same as beads |
| Network | None in Phase 1 | Comes with PostHog integration (Phase 2) |
| Anonymous ID | SHA-256(hostname + OS + arch + random salt) | Stable across sessions, resettable, no raw PII |

## Consent Hierarchy (first match wins)

1. `SHATTER_TELEMETRY=off` env var (CI/automation)
2. `DO_NOT_TRACK=1` env var (cross-tool standard)
3. `~/.config/shatter/telemetry.yaml` → `enabled: false` (user-level persistent)
4. Default: **enabled**

## Event Schema

Common envelope for all events:

```yaml
schema_version: 1
event_id: "uuid-v4"
anonymous_id: "sha256-hex"
timestamp: "2026-03-05T14:30:00Z"
shatter_version: "0.1.0"
os: "linux"
arch: "x86_64"
event_type: "bad_cli_args" | "command_run" | "command_error"
payload: { ... }
```

### Event: `bad_cli_args` (primary use case)

```yaml
payload:
  sanitized_args: ["explore", "--concolic-mode", "<file>.ts"]
  error_kind: "unknown_flag"        # from clap::error::ErrorKind
  attempted_subcommand: "explore"   # if identifiable
  unrecognized_args: ["--concolic-mode"]
```

### Event: `command_run`

```yaml
payload:
  subcommand: "scan"
  language: "typescript"
  function_count: 42
  duration_ms: 12345
  exit_success: true
  flags_used: ["--concolic", "--timeout"]  # flag names only, never values
```

### Event: `command_error`

```yaml
payload:
  subcommand: "explore"
  error_category: "file_not_found" | "permission_denied" | "dir_not_found" | "frontend_spawn" | "timeout" | "io" | "config_parse" | "other"
  # For path-related errors, include the same path_meta as sanitized args:
  path_meta: { exists: false, kind: "unknown", parent_exists: true, parent_writable: true, depth: 3, ext: ".ts" }
  duration_ms: 5432
  exit_success: false
```

The `error_category` is derived from the error chain (e.g., `std::io::ErrorKind::NotFound` → `file_not_found`, `PermissionDenied` → `permission_denied`). The optional `path_meta` gives structural context without revealing the actual path. The `ext` field preserves the file extension from the path that failed.

## Privacy Safeguards: Argument Sanitization

The sanitizer operates on the raw `args` vector (after the binary name). It classifies each argument token and either preserves it, redacts it, or replaces it with a structural placeholder that retains diagnostic value without leaking content.

### Token classification (applied in order, first match wins)

| Rule | Condition | Output | Example in → out |
|---|---|---|---|
| **1. Split `--flag=value`** | Token matches `--\w+=.+` | Split into flag + value; flag preserved, value sanitized by rules below | `--output=/tmp/results` → `--output=<path>` |
| **2. Known subcommand** | Token ∈ `{explore, scan, run, diff, spec-diff, export-tests, build-frontend, stale, telemetry}` | Preserve | `scan` → `scan` |
| **3. Flag name** | Token starts with `-` | Preserve (this is the signal we want — including unknown flags) | `--concolic-mode` → `--concolic-mode` |
| **4. Path-like value** | Contains `/` or `\`, or matches `*.{ts,js,go,rs,json,yaml,yml,toml,md,txt,jsx,tsx,mjs,cjs}` | Replace with `<path>.{ext}` (preserving extension) or `<path>` if no extension | `src/deep/app.ts` → `<path>.ts`, `/home/user/project/` → `<path>` |
| **5. Numeric value** | Matches `^\d+(\.\d+)?$` | Preserve (timeouts, iteration counts — not sensitive) | `30` → `30`, `1.5` → `1.5` |
| **6. Known enum value** | Token ∈ set of known clap value_enum values: `{jest, vitest, gotest, json, markdown, both, text, always, auto, never, error, warn, info, debug, trace, go, rust, typescript}` | Preserve | `jest` → `jest` |
| **7. Everything else** | Default | Replace with `<value>` | `myFunction` → `<value>`, `user@host` → `<value>` |

### Path-value enrichment

When a value is classified as a path (rule 4), the sanitizer probes the filesystem to attach **structural metadata** without revealing the path itself. This is critical for diagnosing "it didn't work" reports where the cause is a bad path.

| Probe | Output field | Values | Example |
|---|---|---|---|
| `std::fs::metadata()` | `path_exists` | `true` / `false` | User targeted a file that doesn't exist → `false` |
| File vs directory | `path_kind` | `"file"` / `"dir"` / `"unknown"` | User passed a directory where a file was expected |
| Parent directory exists | `parent_exists` | `true` / `false` | `--output=<path>.json` with `parent_exists: false` → output dir doesn't exist |
| Write permission on parent | `parent_writable` | `true` / `false` | Permission denied on output directory |
| Depth (count of `/` separators) | `path_depth` | integer | Relative (`2`) vs deeply nested (`8`) — structural signal without content |

These fields are attached to the sanitized arg as metadata, not to the path itself:

```yaml
# In bad_cli_args event:
sanitized_args:
  - value: "<path>.ts"
    path_meta: { exists: false, kind: "unknown", parent_exists: true, parent_writable: true, depth: 3 }
  - value: "--output"
  - value: "<path>"
    path_meta: { exists: false, kind: "unknown", parent_exists: false, parent_writable: false, depth: 5 }
```

This lets us answer: "Users frequently get errors because their output directory doesn't exist — should `shatter` auto-create it?" or "20% of bad-args errors involve non-existent source files — are users confused about which file to target?"

**Probing is best-effort:** if the path is syntactically detected but the process can't stat it (e.g., permission denied on the path itself), all probe fields default to `"unknown"` / `null`. Probing must never cause the telemetry path to error or slow down noticeably.

### Key design rationale

- **Flag names are always preserved** — including unknown ones. This is the primary signal: when a user types `--concolic-mode` (doesn't exist) instead of `--concolic` (exists), the flag name tells us what feature they expected.
- **Flag _values_ are always sanitized** — `--output=/tmp/secret-project` becomes `--output=<path>`. We know they used `--output` with a path, but not what path. But we *do* know whether the target existed and was writable.
- **Extensions are preserved on paths** — knowing someone passed a `.go` file to a command expecting `.ts` is diagnostic. The directory structure is not.
- **Path metadata is safe** — existence, kind (file/dir), writability, and depth are structural properties that don't reveal content or identity. Depth is a count, not a path component.
- **Numbers are safe** — `--timeout 30` and `--max-iterations 100` are not sensitive. Preserving them helps understand typical usage patterns.
- **Enum values are safe** — they come from a fixed set defined in clap, not user content.
- **The `<value>` fallback is aggressive** — function names, identifiers, arbitrary strings all get scrubbed. Better to lose some signal than leak a proprietary function name.

### What clap's ErrorKind tells us (no raw args needed)

For `bad_cli_args` events, clap's `ErrorKind` already classifies the failure:

| `ErrorKind` | Maps to `error_kind` | What we learn |
|---|---|---|
| `UnknownArgument` | `unknown_flag` | User tried a flag that doesn't exist |
| `InvalidSubcommand` | `unknown_subcommand` | User tried a subcommand that doesn't exist |
| `EmptyValue` | `missing_value` | User passed `--flag` without a required value |
| `MissingRequiredArgument` | `missing_required` | User forgot a required positional arg |
| `ValueValidation` | `invalid_value` | User passed a value that failed validation (e.g., non-numeric for `--timeout`) |
| `WrongNumberOfValues` | `wrong_count` | Too many or too few values |
| Other | `other` | Catch-all |

Combined with sanitized args, this lets us answer questions like: "Users frequently try `--concolic-mode fast` — should we add a mode flag?" or "Users pass `.py` files — should we add Python support?"

### What is never collected

- File paths (reduced to extension only)
- Source code content
- Function/variable/class names (scrubbed to `<value>`)
- Usernames, hostnames, IP addresses
- Environment variable values
- Config file contents
- Error messages from command execution (could contain paths/code)
- Flag values (sanitized per rules above)

## Local Queue Design

- **File:** `$XDG_DATA_HOME/shatter/telemetry/events.jsonl`
- **Max size:** 1 MB — oldest events trimmed when exceeded
- **Max age:** 30 days — stale events dropped on flush
- **Locking:** `fs2::FileLock` for concurrent `shatter` invocations; skip if lock not acquired in 10ms
- **Lazy creation:** directory and file created on first write

## Deliverable: Beads Issues

This plan creates beads issues only — no implementation code. The issues capture the full design so any session can pick them up.

### Epic: Telemetry — Anonymous Usage & Bad-Args Reporting

### Issue 1: Core telemetry module (`shatter-core/src/telemetry.rs`)

**Type:** feature, **Priority:** P1

Create `shatter-core/src/telemetry.rs` with:
- `TelemetryEvent` struct (envelope) and `EventPayload` enum (bad_cli_args, command_run, command_error)
- `is_enabled()` — checks consent hierarchy (env vars → config file → default)
- `generate_anonymous_id()` — SHA-256(hostname + OS + arch + random salt), persisted to config
- `sanitize_args(args: &[String]) -> Vec<String>` — see "Argument Sanitization" section above for full rules
- `queue_event(event: TelemetryEvent)` — append to JSONL with file locking + size cap
- `read_config()` / `write_config()` — YAML config at XDG config path
- `show_first_run_notice()` — one-time stderr message, sets `first_notice_shown: true` in config
- Constants: `TELEMETRY_SCHEMA_VERSION`, `TELEMETRY_MAX_QUEUE_BYTES` (1MB), `TELEMETRY_MAX_AGE_DAYS` (30), `TELEMETRY_LOCK_TIMEOUT_MS` (10)

Add `pub mod telemetry;` to `shatter-core/src/lib.rs`.

Dependencies: `uuid = { version = "1", features = ["v4"] }`, `dirs` crate for XDG paths. `sha2`, `rand`, `fs2`, `serde`, `serde_json`, `serde_yaml` already present.

XDG paths: Linux `~/.local/share/shatter/`, `~/.config/shatter/`; macOS `~/Library/Application Support/shatter/`; Windows `%APPDATA%\shatter\`.

**Tests:** consent hierarchy, sanitization (all 7 rules), queue append/size-cap/locking, event serialization round-trip, anonymous ID stability/uniqueness.

### Issue 2: CLI bad-args capture

**Type:** feature, **Priority:** P1, **Depends on:** Issue 1

In `shatter-cli/src/main.rs`:
- Change `Cli::parse()` (line 2874) → `Cli::try_parse_from(std::env::args_os())`
- On `Err(clap_err)`: if telemetry enabled, sanitize args, queue `bad_cli_args` event, then call `clap_err.exit()`
- Map `clap::error::ErrorKind` to `error_kind` field (see ErrorKind table in sanitization section)
- Must not delay error output — queue is a fast local append

### Issue 3: CLI command_run / command_error events

**Type:** feature, **Priority:** P2, **Depends on:** Issue 1

After the `match cli.command { ... }` block (line 2897-3152):
- Record duration via `Instant` before command dispatch
- On `Ok(())`: queue `command_run` with subcommand name, detected language, function count, duration, flags used (names only, from parsed Cli struct)
- On `Err(_)`: queue `command_error` with subcommand, error category, duration
- Error categorization: classify error message into `frontend_spawn | timeout | io | config_parse | other` without including the message itself

### Issue 4: `shatter telemetry` subcommand

**Type:** feature, **Priority:** P1, **Depends on:** Issue 1

Add `CliCommand::Telemetry` variant with sub-subcommands:
- `status` — print consent state, config file location, anonymous ID (truncated), queue file location + event count
- `off` — write `enabled: false` to config
- `on` — write `enabled: true` to config
- `reset-id` — regenerate anonymous ID

Update `demo/walkthrough.sh` to exercise `shatter telemetry status`.

### Issue 5: First-run notice

**Type:** feature, **Priority:** P1, **Depends on:** Issue 1

On first invocation (no telemetry config file exists):
- Print to stderr (one time only):
  ```
  Shatter collects anonymous usage data to improve the tool.
  No file paths, source code, or personal information is collected.
  To disable: shatter telemetry off  (or set SHATTER_TELEMETRY=off)
  ```
- Create config file with `enabled: true, first_notice_shown: true`
- Runs before command dispatch, after consent check
- If `SHATTER_TELEMETRY=off` or `DO_NOT_TRACK=1` is set, skip notice and don't create config

### Issue 6: PostHog integration (Phase 2)

**Type:** feature, **Priority:** P3, **Depends on:** Issues 1-5

Server-side analytics via PostHog. Scope:
- Add `posthog-rs` (or `ureq` + direct HTTP) dependency to `shatter-cli`
- Implement `flush_queue()` — read JSONL, batch POST to PostHog `/capture` endpoint, truncate on success
- 2-second flush timeout, fire-and-forget after command completes
- `SHATTER_TELEMETRY_DEBUG=1` env var to print events to stderr instead of sending
- `SHATTER_TELEMETRY_URL` env var for endpoint override (testing)
- PostHog project setup (cloud free tier initially, self-host option documented)
- Size limits: batch max 100 events per flush, respect PostHog rate limits

## Files Affected (for implementation)

| File | Action |
|---|---|
| `shatter-core/src/telemetry.rs` | **Create** — core module |
| `shatter-core/src/lib.rs` | **Modify** — add `pub mod telemetry;` |
| `shatter-core/Cargo.toml` | **Modify** — add `uuid`, `dirs` |
| `shatter-cli/src/main.rs` | **Modify** — `try_parse_from`, telemetry init, event recording, `Telemetry` subcommand |
| `demo/walkthrough.sh` | **Modify** — add `shatter telemetry status` |

## Verification (for implementation issues)

1. `cargo test -p shatter-core` — telemetry unit tests pass
2. `cargo clippy -p shatter-core -p shatter-cli -- -D warnings` — no warnings
3. `cargo run -- --nonexistent-flag` — prints clap error, event appears in `~/.local/share/shatter/telemetry/events.jsonl`
4. `cargo run -- telemetry status` — shows enabled state, config path, queue stats
5. `SHATTER_TELEMETRY=off cargo run -- --nonexistent-flag` — no event queued
6. `DO_NOT_TRACK=1 cargo run -- --nonexistent-flag` — no event queued
7. `cargo run -- telemetry off && cargo run -- explore foo.ts` — no event queued
8. `bash demo/walkthrough.sh --auto --delay 0` — passes with no errors in the telemetry step
