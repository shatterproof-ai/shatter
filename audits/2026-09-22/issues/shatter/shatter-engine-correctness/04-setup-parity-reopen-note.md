---
slug: setup-parity-reopen-note
kind: reopen-note
title: "Note on closed str-0s76.6: configured setup files are still ignored under --concolic"
priority: P1
type: bug
labels: [concolic, setup, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [concolic-setup-teardown]
existing_id: str-0s76.6
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on closed str-0s76.6: configured setup files are still ignored under --concolic

**Target:** str-0s76.6 (closed). Add a comment. Do not reopen. The work is tracked in the new issue concolic-setup-teardown, so substitute its filed id for the slug when posting.

**Blocked-by note:** this comment points at the new issue, so post it after that issue is filed.

## Comment text

> **Audit 2026-09-22 (finding core-03): closed but not fixed.**
>
> This issue was closed with "All callers updated", but every production caller of `orchestrator::explore` / `explore_with_oracle` still passes `setup_context = None`:
>
> - `shatter-core/src/pipeline_orchestrator.rs:536-548` (7th arg `None`)
> - `shatter-core/src/scan_orchestrator.rs:3103-3115`
> - `shatter-cli/src/commands/observe.rs:179-190`
>
> Setup files come from `.shatter/config.yaml` (`defaults.setup` / `setup_level`, or per-function `setup`); there is no `--setup` flag. `shatter-cli/src/commands/explore.rs:5051-5052` copies the resolved `setup`/`setup_level` into the random explorer's config only, and `orchestrator.rs` contains no teardown call. So `shatter explore <file> --concolic`, with a setup file configured, runs without setup and does not warn. The closing E2E (`shatter-core/tests/e2e_concolic.rs:1555`, `orchestrator_explore_with_setup_context`) injects a context directly into `orchestrator::explore`, so it cannot detect that the pipeline never builds one.
>
> The fix (a pipeline-level setup/teardown helper for both engines, plus a CLI-level `--concolic` E2E driven by a configured setup file, at both `function` and `execution` setup levels) is tracked in **<id of concolic-setup-teardown>**. Line numbers verified on the audit branch (code identical to `56c86168`).
