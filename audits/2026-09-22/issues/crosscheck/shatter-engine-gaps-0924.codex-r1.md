# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** 1242a024331cddf1589674ddbee8b80a6549e7d8080b9b3aac6ea213fc4b146d
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket shatter-engine-gaps-0924 (shatter). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

### Findings

- **MAJOR — The parse-failure problem is stated incorrectly and bundled with separate work.** `parse_config` already returns an error for invalid YAML, and `find_project_config` already surfaces invalid JSON. The draft’s `list-targets` example doesn’t show malformed config being accepted: `list_targets::run` doesn’t load `.shatter/config.yaml` at all. Separate the real gap—commands such as `doctor` that only check config presence—from unknown-key warnings and `--set` handling, and identify which commands actually need to read each config.

- **MAJOR — The acceptance criteria expand “warn on typos” into a large, underspecified config-validation project.** The draft combines YAML/JSON parsing behavior, unknown-key warnings, suggestions, strict mode, `--set` behavior across commands, and shared directory exclusions. Several criteria leave design choices open (“pick one form” of strict mode; “either” reject or warn), so a fresh agent can’t tell which behavior is required. Split unrelated work and decide the user-visible behavior before filing.

- **MAJOR — `--set` handling on `scan` and `run` is a separate CLI flag-scoping issue.** The draft itself says `--set` is only read by `explore` and points to another issue about hiding execution flags. Requiring this issue to change or warn on other commands broadens it beyond detecting unknown config keys. Keep that behavior in its own issue or make the dependency and ownership explicit.

- **MINOR — The exclusion issue’s shared-list criterion may impose unnecessary coupling.** The manifest exclusions are glob patterns in `shatter-core`, while positional glob walking uses directory-name checks in `shatter-cli`; the paths have different semantics. The concrete requirement is to exclude Shatter-managed output from manifest selection. Requiring a shared constant and a test that both paths exclude identical directory names may overconstrain the fix.

- **MINOR — The reproduction fixture mixes the reported bug with additional exclusions.** The finding is about `.shatter/cache/harness/`, but its acceptance test also requires `.git/hooks/x.rs` and `build/gen.rs` to be excluded. Those paths expand the regression’s scope without evidence that they are part of the original observed failure. Test only the reported behavior unless those exclusions are deliberate requirements.

### Verdict

Not ready to file as-is. First correct the parse-failure claim and identify the commands that actually need new handling; then split or narrow the combined work; finally remove unrelated `--set` and exclusion requirements from the config issue.
