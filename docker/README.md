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
directory) and pass any normal `shatter` subcommand:

```sh
docker run --rm -v "$PWD:/work" shatter explore /work/path/to/file.ts
```

The container entrypoint creates `.shatter/` under the mount automatically
if it does not already exist, so first-time runs do not require any host-side
setup. Stdio is preserved, so reports stream back to your terminal.

Other subcommands work the same way:

```sh
docker run --rm -v "$PWD:/work" shatter --version
docker run --rm -v "$PWD:/work" shatter scan /work/src
```

## Notes and follow-ups

- The container currently uses a single read-write bind mount. Splitting
  into a read-only source mount plus a writable artifact mount is tracked
  as future work on str-umw3.
- microVM execution (Firecracker / Cloud Hypervisor), multi-arch builds,
  signing, and registry publishing are out of scope for this slice.
- The image entrypoint defaults to `shatter`, so any flag supported by the
  CLI is reachable as `docker run --rm -v "$PWD:/work" shatter <args>`.
