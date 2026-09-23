---
slug: rust-hint-once-note
kind: note-to-existing
title: "Note on str-qwua7.13: the missing-Rust-frontend hint is still printed twice in explore and is contributor-oriented"
priority: P1
type: note
labels: [rust-frontend, ux, install, audit-2026-09-22]
parent_epic: "(existing issue; parent str-qwua7)"
blocked_by: []
existing_id: str-qwua7.13
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on str-qwua7.13

Target: `str-qwua7.13` (open, P1, "scan and run must agree on a missing frontend; print the remediation once"). Verified open with `bd show` on 2026-09-23. Action: add the comment below. str-qwua7.13 owns printing the remediation once; this note adds explore to its evidence and adds the hint-content requirement so a second issue does not compete with it.

## Comment text

> Audit 2026-09-22 (cli-ux-10): the duplicate hint also affects `explore`, not only `scan`. Exploring a `.rs` target with no `shatter-rust` available printed the hint twice (1,345 bytes of stderr; `audits/2026-09-22/cli-ux-transcripts/rust-explore.err`). The text is `RUST_FRONTEND_INSTALL_HINT` at `shatter-cli/src/helpers.rs:418-424` (used by `check_frontend_availability` at `helpers.rs:497`, which this issue already cites). It is about 600 characters of source-checkout guidance ("the expected state after `cargo build --release --bin shatter` from the workspace root") shown to every user.
>
> Added acceptance checks:
> - The CLI integration tests this issue already requires also cover `explore` on a `.rs` target: exactly one hint occurrence on stderr.
> - The hint is at most two lines: what is missing, and one install command or a pointer to `shatter doctor` (str-qwua7.40) for details. Source-checkout build instructions move to README "Build from source".
>
> Related new issue: **rust-runtime-path-and-doctor** (<filed id>), which covers the next failure a user hits after installing `shatter-rust` (the runtime crate).

## Filing note

The filer script must replace `<filed id>` with the id assigned to rust-runtime-path-and-doctor (05, this bucket).
