# Documentation Map

Where to find what in Shatter's documentation.

## Core Documents

| Document | Role | Audience | Status |
|----------|------|----------|--------|
| [README.md](../README.md) | Quick start, CLI reference, build instructions | **Users and contributors** | Current behavior |
| [SPEC.md](../SPEC.md) | Behavioral specification — how each command, feature, and output format should behave | **Users, contributors, auditors** | Current behavior (living document, updated as functionality changes) |
| [PLAN.md](../PLAN.md) | Architecture vision and implementation roadmap for v2 | **Contributors and architects** | Roadmap — describes planned/in-progress work, not necessarily current state |
| [PROTOCOL.md](../PROTOCOL.md) | JSON-over-stdio wire protocol between core engine and language frontends | **Frontend implementors** | Current behavior (versioned: see `protocol_version` field) |
| [AGENTS.md](../AGENTS.md) | Issue tracking (beads), git workflow, and agent operational instructions | **AI agents and contributors** | Current process |
| [CLAUDE.md](../CLAUDE.md) | Code quality standards, test tiers, completion checklists, task recipes | **AI agents and contributors** | Current process |

## Supplementary Documents

| Document | Role | Audience |
|----------|------|----------|
| [docs/GLOSSARY.md](GLOSSARY.md) | Term definitions used across the project | Contributors |
| [docs/CI-INTEGRATION.md](CI-INTEGRATION.md) | CI pipeline configuration and integration guidance | Contributors |
| [docs/hooks.md](hooks.md) | Git hooks and automation setup | Contributors |

## How to Read These Docs

- **Using Shatter?** Start with [README.md](../README.md) for CLI usage. Consult [SPEC.md](../SPEC.md) for detailed behavior of any command or feature.
- **Building a frontend?** Read [PROTOCOL.md](../PROTOCOL.md) for the wire format, then check existing frontends (`shatter-ts/`, `shatter-go/`) for reference implementations.
- **Contributing code?** Read [CLAUDE.md](../CLAUDE.md) for quality standards and [AGENTS.md](../AGENTS.md) for workflow. Each sub-crate has its own `CLAUDE.md` with component-specific guidance.
- **Understanding the vision?** [PLAN.md](../PLAN.md) describes the v2 architecture and roadmap — but check [SPEC.md](../SPEC.md) to know what's actually implemented today.

## Roadmap vs Reality

[PLAN.md](../PLAN.md) describes the target architecture and features that may not yet be implemented. [SPEC.md](../SPEC.md) is the authoritative reference for current behavior — the `/audit` process compares the codebase against SPEC.md, not PLAN.md. When PLAN.md and SPEC.md disagree, SPEC.md reflects reality.
