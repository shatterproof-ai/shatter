---
slug: rustfmt-reopen-note
kind: reopen-note
title: "Comment on closed str-fr1v: the tree drifted again (58-file churn on 2026-09-21; 107 files unformatted on 09-23); no fmt gate exists"
priority: P2
type: note
labels: [rust, quality-gates, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-fr1v
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Comment on closed str-fr1v

Target: **str-fr1v** (closed). Post as a comment only. Do not reopen.

Comment text:

> Audit 2026-09-22 follow-up. This issue's acceptance was that `cargo fmt --all -- --check` must pass, and it was closed as "Closed". Nothing kept the tree clean afterwards: no Taskfile or CI step runs `cargo fmt --check`, and the rustfmt version floats with `dtolnay/rust-toolchain@stable`.
>
> On 2026-09-21 a crate-wide `cargo fmt -p shatter-core` churned 58 files (+3068/-789), and the agent had to revert it by hand. As of 2026-09-23 (rustfmt 1.9.0-stable), `cargo fmt --all -- --check` lists 97 workspace files, plus 9 in shatter-rust and 1 in shatter-rust-runtime.
>
> A one-time format commit, a pinned rustfmt version, and a `cargo fmt --check` gate in check-static (with forced-gate proof at close) are tracked in `<id of rustfmt-gate>`.

(Filer: replace the `<id of ...>` placeholder.)
