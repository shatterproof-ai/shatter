# Scan artifact filenames embed the mangled absolute source path; deep checkouts hit ENAMETOOLONG and the scan still exits 0

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P2 |
| labels | scan,artifacts,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-8q1b4 |
| source findings | cli-ux-07 |

<!-- body -->
## Problem

Per-function scan artifact names include the absolute path of the source file, so a deep path exceeds the filename limit; the write failure is logged as a warning and the scan succeeds, breaking resume and --from-artifacts.

## Current code facts / evidence

- `shatter-core/src/scan_orchestrator.rs:585-591` `scan_artifact_path` formats `{:05}_{sanitized function_name}.json` with the qualified (absolute-path) name.
- `scan_orchestrator.rs:612-632` `write_scan_artifact_json` only `log::warn!`s on failure.
- Example name: `00001_tmp_claude-1000_-home-ketan-project-shatter_<uuid>_scratchpad_proj_01-arithmetic.ts__classifyNumber.json`; ~230-char source path → `File name too long (os error 36)`.

## Acceptance criteria

- Artifact names use project-relative path segments (directory structure preserved) plus a short hash, bounded length.
- An artifact write failure counts as a scan error (non-zero error count / exit per §2.11).
- Test with a >255-char absolute path succeeds.

## Suggested approach

Change naming; keep reader backward compatibility for one release or bump the artifact schema.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: S

## References

- Audit findings: cli-ux-07 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-8q1b4
