# NOTE TO APPEND to sa-oio: reconcile with the default-deny execution policy; validate the subcommand in run-shatter

## Filing metadata

- tracker/repo: shatter-agents
- action: note to append to existing issue sa-oio
- type: note
- priority: P1
- labels: (none)
- parent: (none)
- dedupe relation: partially-covered (sa-oio open) -> note to append
- source findings: docs-09

## Readiness precheck

- review_mode: local-fallback (this drafting runtime exposed no subagent/Task tool; re-run bento:issue-readiness-check with a fresh reviewer before filing)
- ready: yes
- too_broad: no

<!-- BODY -->
Audit 2026-09-22 (finding docs-09) adds evidence and scope to sa-oio:

- `catalog/skills/add-shatter-target/SKILL.md` (around line 80) says "The wrapper command itself should be `shatter` (no extra flags)". The package.json example is `"shatter": "shatter"` (around lines 92, 112, 125). Bare `shatter` prints usage and exits 2.
- `catalog/skills/run-shatter/scripts/run_targets.py` invokes `npm/pnpm/yarn run shatter` and `task shatter` with no args, so it never validates that a wrapper includes a subcommand.
- No file in shatter-agents mentions shatter's default-deny host-write policy (`--allow-host-writes`, `SHATTER_ALLOW_HOST_WRITES`, sandbox). This policy has been in effect since shatter str-gg9v on 2026-07-09. Executing wrappers are refused without an opt-in.
- Conflict to decide: sa-oio says "Do not inject --allow-host-writes". The audit suggested a canonical body such as `shatter scan . --allow-host-writes -o shatter-review/report.json`. Pick one and document the execution-safety prerequisite explicitly.
- Additional acceptance: run-shatter fails with a clear message when a wrapper has no subcommand, and a plugin test runs a generated wrapper against a fixture.
- The shatter README's Makefile example (`$(SHATTER_BIN)` with no subcommand) is tracked separately in the shatter repo (str-dakf3).
