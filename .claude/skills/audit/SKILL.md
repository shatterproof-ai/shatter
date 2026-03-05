---
name: audit
description: Comprehensive periodic audit of project health, code quality, documentation, issues, methodology, and usability. Produces a prioritized report and creates beads issues for significant findings.
user-invocable: true
---

# Comprehensive Project Audit

Run a 10-phase audit of the Shatter project. **Observation only** — do NOT fix anything during the audit. Write the full report to `audits/YYYY-MM-DD.md` (using today's date) and also print it to the conversation.

---

## Phase 1 — Build Health (~1 min)

Run all build and quality gates. Capture pass/fail and first error lines for each.

1. `cargo test` in workspace root
2. `cargo clippy -- -D warnings` in workspace root
3. `npm test` in `shatter-ts/`
4. `npx tsc --noEmit` in `shatter-ts/`
5. `go test ./...` in `shatter-go/`
6. `go vet ./...` in `shatter-go/`

Produce a summary table:

```
| Language   | Tests | Lint/Vet | Status    |
|------------|-------|----------|-----------|
| Rust       | ...   | ...      | PASS/FAIL |
| TypeScript | ...   | ...      | PASS/FAIL |
| Go         | ...   | ...      | PASS/FAIL |
```

---

## Phase 2 — Code Quality Sampling (~2 min)

- Find the 5 largest `.rs` files in `shatter-core/src/`. For each, spot-check:
  - Does the file have a module-level doc comment (`//!`)?
  - Do public items (`pub fn`, `pub struct`, `pub enum`) have doc comments (`///`)?
- Search for `unwrap()` in `shatter-core/src/` **excluding** `#[cfg(test)]` blocks. Report each occurrence with file:line.
- Search for `: any` in `shatter-ts/src/`. Report each occurrence.
- Check every `.rs` file in `shatter-core/src/` for a `#[cfg(test)]` block. List files missing tests.
- Flag any source file (`.rs`, `.ts`, `.go`) over 500 lines.

---

## Phase 3 — Protocol Consistency (~1 min)

Read protocol type definitions across all 4 languages:
- Rust core: `shatter-core/src/protocol.rs` (or `protocol/` directory)
- TypeScript: `shatter-ts/src/protocol/` or relevant files
- Go: `shatter-go/protocol/`
- Rust frontend: `shatter-rust/src/protocol.rs`

Compare message types, field names, and field types. Report:
- Missing types in any language
- Missing fields
- Type mismatches
- Naming inconsistencies

---

## Phase 4 — Documentation Accuracy (~2 min)

Cross-reference these documents against the actual codebase:
- `SPEC.md` (behavioral specification — **primary focus**)
- `README.md`
- `PLAN.md`
- `CLAUDE.md` (root + each sub-crate)
- `AGENTS.md`
- `GLOSSARY.md`
- `demo/walkthrough.sh`

**SPEC.md verification** (most important — this is the living behavioral spec):
- Every CLI command in SPEC.md section 2 exists in `shatter-cli/src/main.rs` (and vice versa)
- Every flag/option documented in SPEC.md matches the actual clap definitions
- Default values in SPEC.md match the actual defaults in code
- Output format examples in SPEC.md section 5 match actual output structures
- Core concepts in SPEC.md section 3 match actual Rust types and behavior
- Known limitations in SPEC.md section 6 are still accurate (not fixed without updating)
- Changelog in SPEC.md section 7 has entries for recent changes

**General documentation checks**:
- Directory structure references match reality
- Referenced file paths exist
- Build instructions are accurate (commands work, prerequisites listed)
- Task recipes in CLAUDE.md are current
- Walkthrough commands match actual CLI subcommands and flags
- Any documented features that don't exist yet (and vice versa)

---

## Phase 5 — Issue Hygiene (~1 min)

Run `bd` commands to assess issue tracker health:

1. `bd list --status=in_progress` — flag stale in-progress issues (check git log for recent branch activity)
2. `bd blocked` — report blocked dependency chains
3. `bd list --status=open` — check for duplicate or overlapping issues
4. `bd stats` — overall health summary
5. Check for priority inversions (high-priority blocked by low-priority)
6. Check epic health if epics exist (`bd epic list`)

---

## Phase 6 — Foundation Review (~3 min)

Deep read of core concolic components. For each, assess correctness and completeness on a scale: **Solid / Partial / Stub / Missing**.

| Component | Files to Read | What to Assess |
|-----------|--------------|----------------|
| Explorer loop | `explorer.rs` | Is it using symbolic constraints or still purely random/concrete? |
| Solver (Z3) | Z3/solver files | Are symbolic expressions correctly translated? Model extraction correct? |
| Behavior maps | `behavior_map.rs` or similar | Correct clustering by execution path? Complete for test gen? |
| Invariant detection | `invariants.rs` or similar | What's detected? Soundness? Confidence mechanisms? |
| Test generation | `export.rs` or similar | Are generated tests correct and runnable? |
| Input generation | Input gen files | All types handled? Boundary values? Solver constraint respect? |

Produce a component assessment table:

```
| Component          | Status  | Notes |
|--------------------|---------|-------|
| Explorer loop      | ...     | ...   |
| Z3 solver          | ...     | ...   |
| Behavior maps      | ...     | ...   |
| Invariant detection| ...     | ...   |
| Test generation    | ...     | ...   |
| Input generation   | ...     | ...   |
```

---

## Phase 7 — Agent Effectiveness (~1 min)

Review:
- `CLAUDE.md` and `AGENTS.md` for gaps or stale instructions
- Memory files in `.claude/projects/*/memory/` for outdated entries
- Skills in `.claude/skills/` for completeness and accuracy
- Recent git log (last 20 commits) for patterns: broken builds, reverted changes, repeated attempts at the same task

---

## Phase 8 — Usability & Ergonomics (~3 min)

Evaluate from **two distinct perspectives**:

### 8A — Developer Working ON Shatter (contributor experience)

Someone (human or coding agent) who clones the repo to fix bugs, add features, or extend frontends.

**Onboarding & orientation:**
- Can a new contributor go from `git clone` to passing tests by following README.md? Are prerequisites complete and accurate?
- Are file/module names self-explanatory? Can you find the right file to edit for a given task by name alone?
- Are there clear entry points for common tasks? (e.g., grep a CLI command name → find its handler; grep a protocol message → find all language implementations)
- Does `CLAUDE.md` give actionable, step-by-step recipes that produce correct results without clarifying questions?

**Feedback loops:**
- How fast is the inner dev loop? (`cargo test` time, incremental build time)
- Do compiler errors, test failures, and lint warnings point clearly to the fix? Are clippy/tsc/vet suggestions actionable?
- Are test names descriptive of the behavior being tested? (Important for both humans reading test output and agents pattern-matching on failures.)
- Is the test suite deterministic? (Flaky tests waste human time and confuse agents into thinking their changes broke something.)

**Guardrails & safety:**
- Are conventions mechanically enforced (clippy, tsc strict, go vet) or only documented?
- Are there snapshot/golden-file tests that catch unintended output changes?
- Can a contributor safely make a change, run tests, and trust that green means correct?

**Code navigability:**
- Is the module dependency graph obvious from file names and imports?
- Are error types structured (enum variants with context) or stringly-typed?
- Are public APIs minimal and well-documented, or sprawling and unclear?

### 8B — User Running Shatter Against Their Project (end-user experience)

Someone (human or coding agent) who installs shatter and runs it against their own TypeScript/Go codebase to generate specs, find behaviors, or export tests.

**First-run experience:**
- Can a user install and run shatter on a sample file within 5 minutes? Is the happy path obvious?
- Does `shatter --help` make it clear what to do first? Is there a natural command progression (explore → scan → export-tests)?
- Are error messages actionable when something goes wrong? (e.g., missing frontend, unsupported file type, timeout — does the message tell the user what to do next?)

**CLI design:**
- Command naming: are names intuitive? Can a user guess what `explore`, `scan`, `run`, `diff`, `export-tests`, `spec-diff` do?
- Flag overload: are there too many flags on a single command? Are defaults sensible so most users don't need flags?
- Consistency: do similar concepts use similar names across commands? (e.g., timeout flags, output format flags)

**Output quality:**
- Is human-readable output (terminal, markdown specs) scannable and useful? Does it answer "what did shatter find?"
- Is machine-readable output (JSON specs, exported tests) clean and parseable? Is stdout free of informational noise when producing JSON?
- Are generated tests (from `export-tests`) correct, runnable, and readable?
- Do spec files convey useful behavioral information, or are they noisy/repetitive?

**Agent integration (shatter as a tool for coding agents):**
- Can a coding agent invoke shatter, parse its JSON output, and act on the results programmatically?
- Are exit codes meaningful? (0 = success, non-zero = failure, distinct codes for distinct failure modes)
- Is `--spec-json` output stable and well-structured enough for an agent to extract behaviors, invariants, and equivalence classes?
- Can an agent use `spec-diff` to detect behavioral regressions as part of a CI or review workflow?
- Are error messages structured enough for an agent to distinguish "retry-able" failures (timeout) from "fix required" failures (parse error)?

Produce a summary table for each perspective:

```
| Dimension                  | Human | Agent | Notes |
|----------------------------|-------|-------|-------|

8A — Contributor Experience:
| Onboarding / ramp-up       | ...   | ...   | ...   |
| Feedback loop speed         | ...   | ...   | ...   |
| Error/failure actionability | ...   | ...   | ...   |
| Guardrails & safety         | ...   | ...   | ...   |
| Code navigability           | ...   | ...   | ...   |

8B — End-User Experience:
| First-run experience        | ...   | ...   | ...   |
| CLI design & discoverability| ...   | ...   | ...   |
| Output quality (human)      | ...   | ...   | ...   |
| Output parseability (agent) | ...   | ...   | ...   |
| Error actionability         | ...   | ...   | ...   |
| CI/workflow integration     | ...   | ...   | ...   |
```

---

## Phase 9 — Closed Issue Regression Check (~3 min)

Verify that functionality described by closed beads issues is still present in the codebase.

1. Run `bd list --status=closed` to get all closed issues
2. For each closed issue, run `bd show <id>` to get the description
3. Extract verifiable artifacts: file paths, function names, module names, type names, CLI commands, config fields
4. Grep/glob the codebase to verify those artifacts still exist

**Sampling strategy** (for efficiency — don't exhaustively verify all issues):
- All P0 and P1 closed issues (most critical functionality)
- All closed bugs (fixes that could silently regress)
- Random sample of P2 features/tasks

**Categorize each checked issue as:**
- **REGRESSED**: A described function/module/type no longer exists (work was lost)
- **MOVED**: Artifact exists but at a different path (renamed/refactored — informational)
- **PRESENT**: Everything checks out

Produce a regression table:

```
| Issue   | Title                    | Status   | Notes                                      |
|---------|--------------------------|----------|--------------------------------------------|
| str-798 | path_hash fallback fix   | PRESENT  | path_hash() in explorer.rs uses lines_executed |
| str-0oc | Spec output format       | PRESENT  | spec.rs exists with markdown + JSON output |
| str-xyz | Some feature             | REGRESSED| function_name() missing from module.rs     |
```

---

## Phase 10 — Session Retrospective (~3 min)

Analyze recent Claude Code session transcripts for this project to identify anti-patterns and improvement opportunities in both user and Claude behavior. Session transcripts are JSONL files at:

```
~/.claude/projects/*/*.jsonl  # Find the directory matching this project
```

**Sampling strategy**: Analyze ALL session transcripts in the project directory (skip files under 5KB as trivial sessions). The analysis is programmatic (parsing JSONL, counting patterns, extracting signals), not full-content reading, so processing all sessions is feasible and gives a more complete picture of patterns over time.

For each sampled session, parse the JSONL and extract:

### Quantitative Metrics
- Session duration (first to last timestamp)
- Message counts (user vs assistant turns)
- Tool usage distribution (which tools, how often)
- Interrupts (`[Request interrupted by user]` in user messages)
- Consecutive tool failures (same tool failing 2+ times in a row)

### User Anti-Patterns
Look for these signals in user messages:
- **Corrections**: Messages starting with "no", "wrong", "don't", "stop", "that's not", "incorrect" — indicates Claude misunderstood
- **Repeated instructions**: User restating the same request multiple times — indicates Claude didn't follow through
- **Interrupts**: User canceling Claude mid-action — indicates Claude was going in the wrong direction
- **Scope escalation**: User adding requirements mid-task rather than upfront — opportunity for better prompting
- **Missing context**: User having to explain project conventions that should be in CLAUDE.md or memory

### Claude Anti-Patterns
Look for these signals in assistant messages and tool calls:
- **Excessive tool calls**: Sessions with >200 tool calls may indicate thrashing or inefficiency
- **Read-before-edit violations**: Edit tool calls without a preceding Read of the same file
- **Bash over dedicated tools**: Using `cat`, `grep`, `find`, `sed` via Bash when Read/Grep/Glob/Edit tools exist
- **Repeated failed commands**: Same or similar Bash command failing 2+ times consecutively
- **Over-reading**: Reading files that are never referenced again in the conversation
- **Missing parallelism**: Sequential tool calls that could have been parallel (independent reads, independent searches)
- **Large file writes vs small edits**: Using Write for a few-line change instead of Edit
- **Forgotten conventions**: Claude doing something that contradicts CLAUDE.md or AGENTS.md

### Workflow Opportunities
Identify patterns that suggest process improvements:
- **Recurring tasks**: Similar work done across multiple sessions that could be a skill or script
- **Missing skills**: Repeated manual procedures that could be automated as `/skill-name`
- **Memory gaps**: Information repeatedly re-discovered that should be in memory files
- **CLAUDE.md gaps**: Conventions that Claude keeps getting wrong, suggesting missing instructions
- **Prompt patterns**: User prompt styles that consistently lead to better/worse outcomes

### Output for This Phase
Produce two sections:

**User Effectiveness Report**:
- Top 3 opportunities for the user to work more effectively with Claude
- Specific examples from transcripts
- Suggested prompt patterns or workflow changes

**Claude Effectiveness Report**:
- Top 3 anti-patterns observed in Claude's behavior
- Specific examples from transcripts
- Recommended fixes: memory entries to add, CLAUDE.md updates, new skills to create

**Actionable Items** (created as part of post-audit actions):
- Memory entries to add/update (for Claude improvements)
- CLAUDE.md additions (for convention enforcement)
- New skills to create (for recurring workflows)
- User tips document updates (for user improvements)

---

## Output Format

Write a structured report with these sections:

### 1. Executive Summary
- Overall health: **GREEN** / **YELLOW** / **RED**
- Build status (all pass? any failures?)
- Open issue count, blocked count
- 2-3 sentence headline of the most important findings

### 2. Goal Fulfillment Grade
- Letter grade **A–F** assessing how thoroughly the project fulfills its stated goals:
  - Generating machine+human readable behavioral specifications
  - Concrete examples for regression detection
  - Behavior-map-based mocking
- Brief justification: what works end-to-end today, what's partially implemented, what's missing

### 3–11. Per-Phase Sections
Each finding tagged with priority:
- **P1**: Must fix — broken builds, incorrect behavior, security issues, regressions
- **P2**: Should fix — quality gaps, stale docs, missing tests, partial implementations
- **P3**: Nice to have — style issues, minor improvements, polish

### 12. Closed Issue Regression Table
The full regression check table from Phase 9.

### 13. Session Retrospective
User Effectiveness Report and Claude Effectiveness Report from Phase 10.

### 14. Consolidated Action Items
All P1 and P2 findings collected into a single prioritized list, grouped by priority. Include actionable items from Phase 10 (memory updates, CLAUDE.md changes, new skills).

### 15. Issues Created
List of beads issues created by this audit.

---

## Post-Audit Actions

After writing the report:

1. **Dedup check**: For each P1 and P2 finding, run `bd search "<keywords>"` to check if an issue already exists
2. **Create issues**: Create beads issues for P1 and P2 findings that don't already have tracking issues. Tag them appropriately (type=bug for P1 regressions, type=task for quality improvements)
3. **Phase 10 actions**: For actionable Phase 10 findings, create the actual artifacts:
   - Write memory file updates (new entries or edits to existing files in `.claude/projects/*/memory/`)
   - Draft CLAUDE.md additions (as proposed edits in the report — do NOT apply them directly)
   - Create beads issues for new skills to build
4. **Write report**: Save the full report to `audits/YYYY-MM-DD.md` (create directory if needed)
5. **Print report**: Also output the full report in the conversation
6. **Commit report**: Stage and commit the audit report file (`audits/YYYY-MM-DD.md`) with message `audit: YYYY-MM-DD`. Do NOT commit beads issue changes — those are handled by `bd sync`.

**Remember: This is observation only. Do NOT fix any issues found during the audit.**
