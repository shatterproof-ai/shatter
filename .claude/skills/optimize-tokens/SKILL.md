---
name: optimize-tokens
description: Audit and trim auto-loaded context (CLAUDE.md, hooks, plugins) to reduce per-conversation token consumption. Produces a report and applies edits.
user-invocable: true
---

# Optimize Token Consumption

Audit everything that gets auto-loaded into the system prompt, find waste, report it, and trim it. This is a multi-phase procedure: inventory, measure, analyze, report, then edit.

---

## Phase 1 — Inventory Auto-Loaded Content

Identify every source of tokens injected into every conversation. Categorize each as **auto-loaded** (always present) or **on-demand** (loaded only when invoked).

### Sources to check:

1. **CLAUDE.md chain** — root `CLAUDE.md` and all `@`-referenced files (recursive). Read each file, note line count and byte size.
2. **Global user CLAUDE.md** — `~/.claude/CLAUDE.md` if it exists.
3. **Hooks** — `~/.claude/settings.json` and `.claude/settings.local.json`. Check `SessionStart` and `PreCompact` hooks. Run each hook command and measure its output size — this output is injected into context.
4. **Plugins** — check `enabledPlugins` in settings. Plugins may inject their own SessionStart hooks (e.g., beads injects `bd prime`). Look for duplicate injections between user hooks and plugin hooks.
5. **Skill catalog** — count how many skills are listed in the system prompt (each gets a one-line description). Check `.claude/skills/*/SKILL.md` for `user-invocable: true`.
6. **Agent definitions** — `.claude/agents/*/AGENT.md`. These aren't auto-loaded but are listed in the skill catalog if they exist.
7. **Git status snapshot** — always injected, not controllable. Note it for completeness.

Produce a table:

```
| Source                  | Auto-loaded? | Lines | Bytes | Tokens (est) |
|-------------------------|--------------|-------|-------|--------------|
| CLAUDE.md (root)        | yes          | ...   | ...   | ...          |
| @shatter-core/CLAUDE.md | yes         | ...   | ...   | ...          |
| ...                     |              |       |       |              |
| SessionStart hook output | yes         | ...   | ...   | ...          |
| Skill catalog listing    | yes         | ...   | ...   | ...          |
| TOTAL AUTO-LOADED        |             |       |       |              |
```

Estimate tokens as `bytes / 4` (rough average for English text with markdown).

---

## Phase 2 — Analyze for Waste

Read every auto-loaded file line by line. Flag content in these categories:

### Category A: Dead References
- Links to files that don't exist (e.g., `See SPEC.md` when SPEC.md is missing)
- References to features, tools, or configs that aren't set up
- Stale instructions for removed workflows

### Category B: Generic Advice
Content that any capable LLM already follows without being told:
- "Every module has a single, clear responsibility"
- "Name things precisely"
- "Keep functions short"
- "Write tests for public functions"
- Standard coding practices not specific to this project

**Keep only project-specific rules** — things an agent couldn't infer from the codebase alone (e.g., "No `unwrap()` in library code", "pass `cargo clippy` with no warnings").

### Category C: Duplication
- Content repeated across multiple auto-loaded sources (e.g., hook output duplicating CLAUDE.md content)
- Hook output injected more than once (duplicate SessionStart hooks from user config + plugin)
- Project structure tables that duplicate `@`-referenced sub-files
- Information restated in different sections of the same file

### Category D: Verbose Formatting
- Emoji decorations that add tokens but not information (e.g., `🚨 CRITICAL 🚨`)
- Multi-line checklists that could be one sentence
- Motivational preambles ("These are not aspirational — they are requirements")
- Unnecessary markdown structure (### headers for single-line content, tables where a list suffices)
- Code blocks showing obvious/standard commands (e.g., `cargo test` in a Rust project)

### Category E: Rarely-Needed Content
- Detailed setup/installation instructions (one-time activity, not needed every session)
- Historical rationale documents referenced in auto-load
- Recipes for rare tasks (e.g., "Add a new frontend language" in a mature project)

For each finding, note the file, line range, category, and estimated tokens saved.

---

## Phase 3 — Report

Print a report with three sections:

### 1. Current Token Budget
The inventory table from Phase 1.

### 2. Findings
Group by category (A–E). For each finding:
- File and line range
- Category
- What it says (brief quote)
- Why it's waste
- Estimated tokens saved if removed

### 3. Recommended Edits
Ordered by impact (most tokens saved first). For each:
- What to change
- Tokens saved
- Risk assessment (what could break)

---

## Phase 4 — Apply Edits

After presenting the report, ask the user: "Apply these edits?"

If approved, make all edits:
- **CLAUDE.md files**: Use Edit tool to trim content
- **Hook config**: Edit `~/.claude/settings.json` to fix duplicate hooks
- **Sub-crate CLAUDE.md files**: Trim redundant sections

After editing, print a before/after comparison:

```
| File                | Before (lines/bytes) | After (lines/bytes) | Saved |
|---------------------|----------------------|---------------------|-------|
| CLAUDE.md           | ...                  | ...                 | ...   |
| ...                 |                      |                     |       |
| TOTAL               |                      |                     |       |
```

---

## Guidelines

- **Never remove project-specific rules** — only generic advice, dead references, and duplication
- **Never remove @-references** to sub-crate files — they serve a purpose even if their content could be trimmed
- **Never remove skill/agent definitions** — they're on-demand, not auto-loaded
- **Preserve all "What NOT to Do" rules** — these prevent real mistakes
- **Preserve efficiency rules** — these save tokens in practice even if they cost tokens in the prompt
- **Test tiers and task recipes are high-value** — compress but don't remove
- **Update memory files** if the project has a memory directory — record what was changed and why in a `token-efficiency.md` topic file
