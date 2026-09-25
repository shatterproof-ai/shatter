# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** d599bfa37ed34e3c34fb024f06dde6a97ccfec4e87640f0f499eba0edd2ab7c4
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket shatter-agents-plugin (shatter-agents). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

**Not ready to file as-is.** The withdrawal defects are supported, but several proposed fixes have contradictory or incomplete acceptance criteria. I checked shatter-agents at `119b807` and the available shatter audit worktree; live tracker status was not independently verified.

1. **MAJOR — 06: `list-targets` does not return package roots.**  
   The engine returns one `project_root` and selected source-file entries, whereas `run_targets.py` discovers package roots and their wrappers. Against the existing `mixed-repo` fixture, the audit binary returns `selected: []` while `tests/test_run_targets.py:39` expects three packages; define the mapping and preservation requirements before requiring replacement of the walker.

2. **MAJOR — 06: The proposed test cannot prove engine discovery excludes generated harnesses.**  
   The engine’s default exclusions do not include `.shatter`, and a canned JSON response merely assumes the desired exclusion. A fixture containing only `Cargo.toml` is also insufficient for source-file discovery; require a real-binary test containing generated Rust source before claiming this closes sa-d8j.

3. **MAJOR — 05: Contract-test coverage conflicts with retaining unreleased designs.**  
   The test must validate every invocation in `catalog/**/*.md`, while 01 and 03 explicitly allow unsupported designs to remain there with an unreleased marker. Specify that executable-contract validation covers published skills, with explicit handling for proposals, negative examples, and withdrawn material; otherwise completing the withdrawal can still leave CI failing.

4. **MAJOR — 05: Help validation cannot catch the recipe defect cited as justification.**  
   Checking subcommands and flags cannot establish that recipes are discovered, configurations are consumed, or per-recipe execution occurs. Narrow the promised protection to CLI syntax and track behavioral verification separately; also separate the independently useful packaging-metadata mechanism or explain its required connection.

5. **MAJOR — 03: Acceptance permits implementation that the stated scope cannot deliver.**  
   One criterion allows keeping per-recipe runs by implementing them in `run_targets.py`, despite the issue establishing that engine support is absent and excluding that implementation. Choose withdrawal/design labeling for this issue, and link the separate engine implementation work instead of leaving two substantially different completion paths.

6. **MAJOR — 09: The path-check acceptance criterion rejects valid skill references.**  
   Requiring every relative path to exist inside its own skill directory contradicts the explicitly allowed shared taxonomy and existing sibling-script reference in `shatter-gaps/SKILL.md:41`. It also captures downstream project paths and generated outputs; define which references are bundled resources and allow resolution within the plugin payload.

7. **MAJOR — 01: Withdrawal leaves affected users’ installed hooks broken.**  
   Removing the published recipe does not alter a hook already copied into `.git/hooks/pre-commit`, so those users remain unable to commit after updating. Add recovery instructions and a check that removing the documented hook stanza restores commits while preserving unrelated hook logic.

8. **MINOR — 08: The draft omits an existing native-wrapper fallback.**  
   `wire-shatter-ci/SKILL.md:86–91` already instructs agents to invoke native wrappers when the helper is not vendored, and verification also checks the integrated-target command. Describe the defect as conflicting template/companion guidance and insufficient verification, rather than claiming the skill invariably generates a missing-script invocation.

9. **MINOR — 09: Suggested companion-file support already exists.**  
   `scripts/build-plugins:123` already collects `references/`, and lines 177–184 copy it into payloads. The missing work is supplying and resolving the taxonomy reference, not adding support for that directory.

10. **MAJOR — Bundle: Engine evidence and cross-repository references are not independently reproducible.**  
    “Audit HEAD” supplies neither an immutable engine commit nor binary provenance, and names such as `retire-snapshot-diff` are draft slugs without tracker links. Put the engine SHA and resolved references into each affected issue before filing so a fresh agent can reproduce the claims without this bundle.

The three highest-value fixes are to establish a valid discovery contract for 06, reconcile executable-versus-design coverage across 01/03/05, and replace ambiguous verification requirements with scoped checks and immutable evidence.
