# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** 6e41b462228c5ce66388c3c595f99504ad6cd6151ac7e2572508945fb7640d8f
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket shatter-tracker-and-beads (shatter). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

**Not ready to file as-is.** Repository inspection confirms the stale JSONL consumers and obsolete `bd sync` guidance. Live tracker verification was unavailable: `bd --readonly show` could not connect, and the read-only filesystem prevented server startup; tracker-record comparisons below use the committed snapshot.

1. **BLOCKER — Draft 07 creates a filing prerequisite cycle.**  
   Report publication must finish before the single D6 filer runs, but publication is itself an unfiled issue and requires the Dolt procedure supplied by unfiled draft 02. Define an explicit bootstrap sequence, including which prerequisite work may be tracked and executed before the bulk filer.

2. **MAJOR — Draft 01 equates historical reversions with corruption.**  
   Returning to a JSONL value can be an intentional edit, and history alone does not establish the “last intended state.” Separate candidate detection from confirmed import damage, require attribution or maintainer adjudication before repair, and allow an “unresolved” classification instead of forcing every difference into “expected” or “repaired.”

3. **MAJOR — Draft 02 does not prove JSONL import retirement across all entry points.**  
   Its acceptance checks cover checkout, but the repository also has a beads-managed `post-merge` hook, and the chosen configuration’s persistence across fresh clones is unspecified. Require verification of every applicable import entry point and a documented bootstrap that prevents fresh clones from restoring the old behavior.

4. **MAJOR — Draft 03’s completion condition contradicts its exclusions.**  
   It requires zero nonhistorical `bd sync` mentions in AGENTS.md while excluding landing-prose mentions assigned to str-qwua7.23, without depending on that work. Assign those lines to one issue and encode the ordering, or narrow the completion condition.

5. **MAJOR — Draft 03 leaves cleanup’s failure behavior incomplete.**  
   The existing script has both `--skip-bd` and a last-minute per-branch claim check, beyond the bulk in-progress list. Specify what happens when remote pull fails but local queries succeed, whether `--skip-bd` remains permitted, and require tests preserving protection for newly claimed branches.

6. **MAJOR — Draft 03’s command-validation rule would reject legitimate documentation.**  
   AGENTS.md deliberately mentions nonexistent commands such as `bd claim`, `bd assign --self`, and `bd start` to prohibit them. Running `--help` for *every named subcommand* would fail on these warnings; restrict validation to designated executable examples and test negative examples separately.

7. **MAJOR — Draft 08’s proposed close of str-qwua7.12 leaves a substantive requirement unresolved.**  
   The committed issue requires mixed-success multi-target runs to exit 1, while [the current explore test](/home/ketan/project/shatter/shatter-cli/src/commands/explore.rs:7075) explicitly expects success for partial failure. Four single-error examples do not justify closure: reconcile that requirement and account for each remaining acceptance criterion before closing.

8. **MAJOR — Draft 08 mistakes a newly created branch for landed work.**  
   A new `<id>-*` branch created at `origin/main` already satisfies the proposed ancestry test before its first commit. Require affirmative landing evidence, with fixtures for new branches, uncommitted work, reopened issues, and deleted feature branches—the normal cleanup case.

9. **MAJOR — Drafts 08/09 introduce an undefined reporting severity.**  
   Drift-patrol currently supports PASS, FAIL, PENDING and SKIP; its existing hygiene check already FAILs stale in-progress issues after 14 days. Specify WARN’s reporting and exit-code behavior, whether it replaces that existing FAIL, and integration tests beyond individual check functions.

10. **MAJOR — Draft 07 duplicates existing work without resolving conflicting acceptance criteria.**  
    The committed str-qwua7.22 acceptance text orders filing before landing, whereas draft 07 requires landing before filing; str-qwua7.44 already owns the Audits index section. “Coordinate” leaves competing instructions active: explicitly transfer or supersede those criteria, and separate report recovery/publication from the recurring workflow rewrite.

11. **MAJOR — Draft 07 requires a deletion cause that may be unrecoverable.**  
    Missing refs and surviving objects establish recoverability, not who deleted the branch or why. Make investigation bounded and permit “cause undetermined” with evidence searched, so publishing recoverable reports cannot remain blocked indefinitely.

12. **MAJOR — Draft 10 neither preserves the complete goal nor assigns ownership.**  
    The cited kapow and pickpackit memories explicitly allow 80% coverage for UI code, which the blanket ≥90% framing omits. Preserve those thresholds, attach the necessary recipes and eligible-set definitions in durable evidence, and require named responsibility plus clear per-project child completion conditions.

13. **MINOR — Drafts 01/02 present inconsistent measurements as one established root cause.**  
    They alternate between 1,773 imported issues and 1,733 JSONL records, and between roughly six minutes, 2m59.7s, and a 300-second cutoff without identifying distinct runs. Attach command, binary, working directory and timing output for each; low CPU establishes waiting but does not identify what caused the wait.

The three highest-value fixes are: resolve the filing/dependency sequence; replace unsafe repair, closure and landing-detection rules with evidence-based criteria; and give overlapping deliverables one owner while making their acceptance conditions consistent.
