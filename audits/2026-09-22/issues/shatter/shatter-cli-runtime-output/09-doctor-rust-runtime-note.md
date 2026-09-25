---
slug: doctor-rust-runtime-note
kind: note-to-existing
title: "Note on str-qwua7.40: also report the shatter-rust-runtime crate location in doctor's Rust section"
priority: P2
type: note
labels: [rust-frontend, doctor, install, audit-2026-09-22]
parent_epic: "(existing issue; parent str-qwua7)"
blocked_by: []
existing_id: str-qwua7.40
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.40

Target: `str-qwua7.40` (open, P2, "`shatter doctor`: report whether shatter-rust is resolvable, from where, and its version/protocol match"). Verified open with `bd show` on 2026-09-23. Action: add the comment below. str-qwua7.40 keeps ownership of doctor's Rust section, and all of its existing acceptance checks stand (resolved path and how it was found, `frontend_version`/`protocol_version` from a handshake with a mismatch warning, warning-only exit unless Rust is required, same resolver as scan/explore, fake-binary unit test).

## Comment text

> Audit 2026-09-22 (cli-ux-10) adds one check to this issue's Rust section. A resolvable `shatter-rust` is not enough: an installed or copied binary fails every Rust execution with `cannot locate shatter-rust-runtime crate; set SHATTER_RUNTIME_PATH`, because `find_runtime_crate_path()` (`shatter-rust/src/executor.rs:1198-1223`) only honours `SHATTER_RUNTIME_PATH` or a `shatter-rust-runtime/` sibling within five ancestors of the `shatter-rust` executable. Doctor showed all green in that state (`audits/2026-09-22/cli-ux-transcripts/doctor.out`, exit 0).
>
> Added acceptance checks:
> - The Rust section also reports the runtime-crate location: the `SHATTER_RUNTIME_PATH` value (and whether `<value>/Cargo.toml` exists), else the auto-discovered path from the executable-ancestor walk, else "not found" with a one-line fix naming `SHATTER_RUNTIME_PATH`. The check reuses the same lookup logic as the frontend (move it into a shared helper or query it over the handshake) so doctor cannot disagree with execution.
> - Severity follows this issue's existing rule: a missing runtime crate is a warning, and a failure only when Rust is required. The require-flag spelling must match whatever str-qwua7.13 settles on (it proposes `--require-frontend <lang>`, this issue proposes `--require-rust`); pick one before implementing.
> - Test: a relocated `shatter-rust` with `SHATTER_RUNTIME_PATH` unset makes doctor report "runtime crate: not found"; with the variable set to the real crate it reports the path. The first case shows no runtime line (all green) on current main.
>
> The runtime error dedup, the `rust` failure-impact row and the env-var docs are tracked in the new issue **rust-runtime-path-and-doctor** (<id of rust-runtime-path-and-doctor>). Toolchain and sandbox/host-write readiness in doctor are tracked in **doctor-execution-readiness** (<id of doctor-execution-readiness>).

## Filing note

The filer script must replace each `<id of slug>` placeholder with the ids assigned to rust-runtime-path-and-doctor (05) and doctor-execution-readiness (11), both in this bucket.
