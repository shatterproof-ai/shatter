---
repo: shatter
type: feature
priority: 2
labels: shatter-rust, docs, build
existing: none
---
# Embed the Rust frontend in the shatter binary like TS/Go
## Decision (2026-09-06)
Embed the Rust frontend in the shatter binary the way TS and Go are embedded (option 2). Until it lands, `doctor` reports Rust resolution (str-qwua7.40). SPEC §1.3 gains a Distribution column reading embedded for all three once done.

## Problem
TS and Go frontends are embedded in the `shatter` binary; Rust ships as a separate `shatter-rust` executable that must be on PATH or next to a source checkout. QUICKSTART's build-from-source path (`cargo build --release`) therefore yields a binary that silently skips `.rs` targets; the captured audit environment had `STATUS skipped_by_unavailable_frontend` for every Rust target. SPEC §1.3 says "Supported" with no mention of the distribution difference.

## Current code facts
- `shatter-cli/src/embedded_frontend.rs` (TS bundle) and `embedded_go_frontend.rs` (Go binary) are produced by `shatter-cli/build.rs` (:92-157 runs `npm run bundle`, copies bundles into `OUT_DIR`).
- Rust discovery: `shatter-cli/src/helpers.rs` looks for `shatter-rust/target/{release,debug}/shatter-rust` relative to cwd, then PATH; `install.sh:15-22` copies the sibling binary from the release archive.
- README.md:88-117 "Enabling Rust scans from a source build" explains this (contributor-facing); QUICKSTART.md:36-46 build-from-source does not; QUICKSTART "Notes" mentions `cargo build --manifest-path shatter-rust/Cargo.toml` only at the end.
- `scan` skips `.rs` with a 600-char warning; `run` aborts (p1-11). `--require-rust` exists on explore/scan. `shatter-rust` is excluded from the Cargo workspace (root `Cargo.toml`), which is why a single `cargo build` cannot produce it.

## Options
1. **Document now (proposed default)**: SPEC §1.3 gains a "Distribution" column (embedded / embedded / sibling binary) and §7 a limitation row; QUICKSTART §1 build-from-source shows `cargo build --release --manifest-path shatter-rust/Cargo.toml` inline with a one-line "why"; README's section is linked, not duplicated (p2-31b).
2. **Embed**: `build.rs` invokes `cargo build --manifest-path shatter-rust/Cargo.toml` and embeds the binary like Go (`embedded_rust_frontend.rs`); remove sibling discovery; release archive shrinks to one file. Cost: nested cargo in build.rs, doubles cold build time, needs the same staleness hashing as the Go embed (str-o09e).

## Acceptance checks
- Decision recorded; docs or embedding implemented accordingly; `doctor` reports resolution (p2-28c) either way.

## Scope
In: distribution decision + docs. Out: `scan`/`run` behavior alignment (p1-11), `doctor` check (p2-28c), Rust parity gaps.

## Size
small (docs) / large (embedding).

## Provenance
Audit 2026-09-04, section 11, action item 28; evidence audits/2026-09-04/design-foundation.md §2.5, §2.8; usability-ui.md §7.
