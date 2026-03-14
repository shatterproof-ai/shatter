# Plan: `demo/walkthrough-docker.sh` — Container-based walkthrough

## Context

The existing `demo/walkthrough.sh` runs the full Shatter pipeline locally (requires Rust toolchain, Node.js, Go, libclang). We need a container mode that runs the walkthrough inside the distributable Docker image (`Dockerfile` at repo root), validating the actual artifact users receive. CI uses container mode; developers continue using local mode.

## Approach

Create a single new script `demo/walkthrough-docker.sh` that:

1. **Builds (or reuses) the Docker image** — `docker build -t shatter-walkthrough .` (skipped if `--image` flag provides a pre-built image name)
2. **Runs walkthrough steps inside the container** — mounts `examples/` as a read-only volume at `/repo/examples`
3. **Reuses the same step structure and error summary format** as `walkthrough.sh`

### Key design decisions

- **Not a wrapper around `walkthrough.sh`** — the local walkthrough uses `cargo run`, but the container has a pre-built `/usr/local/bin/shatter`. The docker script sets `SHATTER="shatter"` (the binary directly) and reimplements the step runner to execute via `docker run`.
- **Each step = one `docker run` invocation** — stateless, simple, no persistent container. The `SHATTER_CACHE_DIR` is a Docker volume shared across steps for the caching test (steps 6, 38-41).
- **Subset of steps** — some steps don't apply in container mode (e.g., `cargo build` for shatter-rust, `--dry-run` which needs cargo). The script runs the applicable subset and documents skipped steps.
- **Same error tracking** — `ERROR_LOG`, `STEP_ERRORS`, and the `ERROR SUMMARY` footer match `walkthrough.sh` exactly.

### Script structure

```
demo/walkthrough-docker.sh
├── Arg parsing: --image NAME, --auto (default), --interactive, --delay N, --dry-run, --help
├── Docker availability check (graceful error if docker not found)
├── Image build (or pull if --image provided)
├── Helper: docker_run() — wraps `docker run --rm -v examples:/repo/examples:ro ...`
├── Steps (same banner/run_cmd/pause pattern as walkthrough.sh)
│   - Reuses the EXAMPLES/GO_EXAMPLES/RUST_EXAMPLES arrays
│   - SHATTER="shatter" (the container binary)
│   - Each run_cmd invokes docker_run instead of local cargo
└── Error summary (identical format)
```

### Files to create/modify

| File | Action |
|------|--------|
| `demo/walkthrough-docker.sh` | **Create** — new script (~200 lines) |

No other files need modification.

## Verification

1. `bash -n demo/walkthrough-docker.sh` — syntax check
2. `file demo/walkthrough-docker.sh` — confirm executable bit
3. Verify script handles missing Docker gracefully (check output when `docker` not in PATH)
4. Run `/pre-completion` skill
