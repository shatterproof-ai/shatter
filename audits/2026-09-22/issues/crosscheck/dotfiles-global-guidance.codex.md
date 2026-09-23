# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** dff5f5968b4341f5b191291ef538e828871a19ba3af840ae131c9b545a93d01f
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket dotfiles-global-guidance (dotfiles). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

Not ready to file as-is. I checked dotfiles at `81f35e1`; GitHub access failed, so the claimed overlap and closure history remain independently unverified.

1. **BLOCKER — 03 contradicts D4’s prohibition on bypass guidance.**  
   The acceptance criteria explicitly permit listing `--no-verify`, hooksPath overrides, and skip variables as secondary options, while D4 and the issue’s own exclusion say no bypass guidance anywhere. Resolve that contradiction before an implementer chooses which instruction to follow.

2. **MAJOR — 10 treats every classifier denial as permission to change execution paths.**  
   A denial does not establish that another tool is authorized; the cited denial involved `push --no-verify` to main. Require interpreting the denial and distinguishing an already-authorized sanctioned workflow from an action that still requires approval.

3. **MAJOR — 01, 03 and 10 use render status as false proof of rule delivery.**  
   [agents-sync.sh](/home/ketan/dotfiles/codex/agents-sync.sh:74) compares the base, snapshot and live `AGENTS.md`; it does not inspect referenced leaves, and `render vs snapshot: ok` can coexist with live drift. Require assertions on the deployed rule content and fresh-session evidence for both runtimes.

4. **MAJOR — 01’s suggested composition breaks the existing render/adopt contract unless all operations change.**  
   Appending `core.md` only during render would make the generated snapshot differ from the base, while adoption currently copies the entire live file back into the base. Specify composition-aware status and adoption behavior, with round-trip tests preventing duplicated core content.

5. **MAJOR — 01 makes required leaves optional without requiring equivalent coverage.**  
   The proposed minimum core omits branches-and-worktrees and read-before-designing, although both are currently Required Loads. Preserve those load requirements or explicitly require their essential constraints in the injected core before declaring leaves optional.

6. **MAJOR — 01’s before/after metric cannot measure its proposed fix.**  
   The scan counts agent tool calls mentioning guidance paths; automatic SessionStart injection and rendered instructions require no such calls. Measure actual context delivery with a defined sample and success threshold, rather than treating unchanged read-call counts as evidence of failure.

7. **MAJOR — 02’s notification counter misses the documented alternating loop.**  
   Resetting the counter on every other tool call permits unlimited `ReadNotifications → unrelated tool → ReadNotifications` polling. Add a regression fixture representing the motivating sequence and define whether the guard detects that behavior or merely four adjacent calls.

8. **MAJOR — 08’s prescribed regression test is both ineffective and unisolated.**  
   Running every rendered hook with real `HOME` invokes unrelated hooks, while the restricted PATH makes `bd prime` fail independently; empty stdin also skips the summary/name paths, and missing Python scripts exit 2 with “can't open file,” escaping the proposed assertions. Use isolated dependencies and representative event payloads, with explicit checks that each affected script was reached.

9. **MAJOR — 04 has two incompatible definitions of completion.**  
   “A staleness check exists” can apparently be satisfied merely by filing an issue elsewhere, leaving the advertised protection absent. Separate update configuration from checker implementation, or explicitly make the checker a blocking dependency with an implementation-based closing condition.

10. **MAJOR — 07 mistakes unused allowlist entries for failed validation.**  
    A valid allowlist entry need not match anything in a particular repository or run; requiring every entry to match creates false failures. Restrict this requirement to declarations whose contract explicitly requires coverage, and include a valid empty/nonmatching case.

11. **MAJOR — 11’s claimed contradiction is not demonstrated.**  
    Permission to use shell commands does not contradict a preference for dedicated tools, and aggregate usage counts do not establish that dedicated tools could express those operations. Quote the actual conflicting harness requirement or reframe this as a requested policy relaxation.

12. **MAJOR — The standalone tickets lack resolved dependencies and decision context.**  
    Several require a core file from an unfiled slug while declaring no blockers, and D4/D6 live mainly in the bundle wrapper; 02 and 04 additionally direct future agents to file issues despite D6. Require the filer to substitute concrete cross-links, carry applicable decisions into each body, and establish ownership and landing order for shared deliverables.

13. **MINOR — 02 names a test runner that does not discover Python tests.**  
    [claude/tests/run.sh](/home/ketan/dotfiles/claude/tests/run.sh) discovers only `test_*.sh`, so adding `test_wait_guard.py` alone will not execute it. Explicitly include runner integration or name the Python test command.

14. **MINOR — 04 conflates the two plugins’ source versions.**  
    The source marketplace declares shatter `0.1.12` and refute `0.1.2`, not a shared `0.1.12`. State each plugin’s installed and expected version separately.

The three highest-impact fixes are to reconcile the bypass/denial policies, replace misleading completion checks with isolated tests and runtime evidence, and resolve cross-ticket dependencies before filing individual bodies.
