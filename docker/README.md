# Shatter container image

A self-contained OCI image that bundles `shatter` and its TypeScript, Go, and
Rust frontends with all their build/runtime dependencies (Z3, Node.js, Go).
This is the first thin slice of [str-umw3](../docs/) — containerized
whole-tool execution — and lets you run Shatter against a project on disk
without installing any of its toolchains locally.

## Build the image

From the repo root:

```sh
docker build -t shatter .
```

The build is multi-stage: the builder stage compiles `shatter-cli` (which in
turn bundles `shatter-ts` via `build.rs` and compiles `shatter-go`), then the
runtime stage copies just the binaries onto `node:22-slim`.

## Run against a project

Bind-mount the directory you want to analyze at `/work` (the image's working
directory) and pass any normal `shatter` subcommand. On Linux, pass
`--user "$(id -u):$(id -g)"` so output files are owned by your host user
instead of root:

```sh
docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/work" shatter explore /work/path/to/file.ts
```

The container entrypoint creates `.shatter/` under the mount automatically
if it does not already exist, so first-time runs do not require any host-side
setup. Stdio is preserved, so reports stream back to your terminal.

Other subcommands work the same way:

```sh
docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/work" shatter --version
docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/work" shatter scan /work/src
```

> **Why `--user`?** Without it the container runs as root (UID 0). Files
> written into the bind-mounted volume (`.shatter/` caches,
> `shatter-artifacts/` outputs) end up owned by `root:root` on the host,
> requiring `sudo` to delete or edit. Passing `--user` maps the container
> process to your host UID/GID so artifacts have normal ownership.
>
> On macOS with Docker Desktop this is not needed — the VM translates
> file ownership automatically. The flag is harmless there, so the examples
> above work on both platforms.

## Split-mount mode (read-only source)

For defense in depth — especially when running Shatter against untrusted
code — mount the source tree read-only and only the output paths read-write:

```sh
docker run --rm \
  --user "$(id -u):$(id -g)" \
  -v "$PWD:/work:ro" \
  -v "$PWD/.shatter:/work/.shatter" \
  -v "$PWD/.shatter-cache:/work/.shatter-cache" \
  -v "$PWD/shatter-artifacts:/work/shatter-artifacts" \
  shatter explore /work/path/to/file.ts
```

The three writable mount targets are:

| Path | Purpose |
|---|---|
| `.shatter/` | Project config and scratch data |
| `.shatter-cache/` | Custom-built frontend binaries |
| `shatter-artifacts/` | Generated tests, recorded mocks, and reports |

The entrypoint pre-creates these directories if they do not exist, so you
do not need to `mkdir` them on the host before the first run.

If the source is mounted read-only and Shatter attempts to write outside
the writable subtrees, the write fails with a "Read-only file system"
error rather than silently corrupting data.

> **When to use split mounts vs. the simple mode:** The simple single-mount
> mode (`-v "$PWD:/work"`) is fine for trusted projects and local development.
> Use split mounts when analyzing third-party or adversarial code, or when
> you want to guarantee that Shatter cannot modify your source files.

## Notes and follow-ups

- microVM execution (Firecracker / Cloud Hypervisor), multi-arch builds,
  signing, and registry publishing are out of scope for this slice.
- The image entrypoint defaults to `shatter`, so any flag supported by the
  CLI is reachable as `docker run --rm --user "$(id -u):$(id -g)" -v "$PWD:/work" shatter <args>`.
