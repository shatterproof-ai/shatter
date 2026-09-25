---
slug: scan-artifact-filenames-abs-path
kind: new
title: "Scan artifact filenames embed the mangled absolute source path; deep checkouts hit ENAMETOOLONG and the scan still exits 0"
priority: P2
type: bug
labels: [scan, artifacts, resume, audit-2026-09-22]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Scan artifact filenames embed the mangled absolute source path; deep checkouts hit ENAMETOOLONG and the scan still exits 0

## Problem

Each per-function scan artifact is named `{index:05}_{sanitized qualified name}.json`. The qualified name contains the absolute path of the source file. In a deep checkout (CI workspaces, worktrees under long home paths, temp dirs) the file name exceeds the 255-byte filename limit and the write fails. The failure is only `log::warn!`ed. The scan exits 0 and reports success, but the artifacts that `--resume` and `--from-artifacts` depend on are missing. The names also change when the same project is checked out at a different path, so artifacts are not portable between machines.

## Evidence

Line numbers re-checked against `56c86168` (branch `audit-2026-09-22`):

- `shatter-core/src/scan_orchestrator.rs:586-592` `scan_artifact_path` formats `"{:05}_{}.json"` with `sanitize_artifact_component(function_name)`, where `function_name` is the qualified (absolute-path) id.
- `scan_orchestrator.rs:612-640` `write_scan_artifact_json` returns after `log::warn!` on every failure (create dir, serialize, temp write, rename), and nothing is counted as an error.
- Example name from a real run (`audits/2026-09-22/cli-ux-transcripts/ts-scan.err`): `00001_tmp_claude-1000_-home-ketan-project-shatter_<uuid>_scratchpad_proj_01-arithmetic.ts__classifyNumber.json`.
- Repro (`audits/2026-09-22/cli-ux-transcripts/scan-deep.{err,out}`): a source path about 230 characters deep gives `[warn] failed to write scan artifact temp file for …::f: File name too long (os error 36)`, and the scan exits 0.

## Acceptance criteria

- [ ] Artifact names are built from the project-relative source path, with directory structure preserved as subdirectories or as bounded segments, plus the function name and a short stable hash. Every file-name component is at most 255 bytes, whatever the checkout depth. The same project at two different absolute paths produces identical artifact names.
- [ ] A failure to write an artifact counts as a scan error. It appears in the error count and the report, and the exit code follows SPEC §2.11.
- [ ] Test: scan a fixture whose absolute path is more than 255 characters (create it under a temp dir). All artifacts are written, and the scan's error count is 0. A second test makes the artifact dir unwritable and asserts a non-zero error count and exit code. At close, show the long-path test failing on current `main` and passing after the fix.
- [ ] `--resume` and `--from-artifacts` read the new names. Old-name artifacts are either still readable for one release or rejected with a message telling the user to re-scan (state which in SPEC §8).
- [ ] `task affected` passes, and its `Gates selected` output is recorded.

## Suggested approach

Compute a relative id once, the same key that behavior-map-cache-keys needs, and derive the file name from it: `functions/<rel dir>/<index>_<fn>_<hash8>.json`, truncating long segments. Change `write_scan_artifact_json` to return `Result` and propagate it into the per-function outcome.

## Out of scope

- Mixed-language sub-scans deleting each other's artifacts (mixed-language-scan-deletes-artifacts).
- Explore artifact naming (`persist_root/<file>/<line>_<fn>`), which already uses the file path as a directory component. Check it for the same length issue, but fix it here only if it is trivially the same helper.

## Priority

P2: silent data loss in deep checkouts, and the exit status hides it.

## Type

bug

## Dependencies

- Blocked by: none.
- Related: mixed-language-scan-deletes-artifacts (same artifact dir; do not conflict), behavior-map-cache-keys (shared relative-key helper), str-8q1b4 (closed; resume parity).

## References

Audit 2026-09-22 finding cli-ux-07 (verified P2). Source draft: `drafts/shatter-code/29-scan-artifact-filenames-abs-path.md`.
