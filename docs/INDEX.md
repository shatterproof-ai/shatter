# Documentation Map

Where to find what in Shatter's documentation.

## Core Documents

| Document | Role | Audience | Status |
|----------|------|----------|--------|
| [README.md](../README.md) | High-level product overview and user entry point | **Users** | Current behavior |
| [QUICKSTART.md](../QUICKSTART.md) | Copy-paste first run for end users | **Users** | Current behavior |
| [docs/PROJECT-LAYOUT.md](PROJECT-LAYOUT.md) | Project directories, config files, caches, artifacts, and legacy path notes | **Users and contributors** | Current behavior |
| [CONTRIBUTING.md](../CONTRIBUTING.md) | Contributor setup, build/test workflow, and navigation to agent/process docs | **Contributors** | Current behavior |
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
| [docs/execution-adapters.md](execution-adapters.md) | Long-term architecture reference for framework-specific execution adapters, heuristics, composition, and cross-language extension points | Contributors and architects |
| [docs/validation/kapow-refute-agent-workflow.md](validation/kapow-refute-agent-workflow.md) | Refute wrapper and smoke workflow for agents validating Kapow | Agents and contributors |

## How to Read These Docs

- **Using Shatter?** Start with [README.md](../README.md), then run through [QUICKSTART.md](../QUICKSTART.md). Use [docs/PROJECT-LAYOUT.md](PROJECT-LAYOUT.md) to understand what Shatter writes in your project, and consult [SPEC.md](../SPEC.md) when you need precise command or output behavior.
- **Building a frontend?** Read [PROTOCOL.md](../PROTOCOL.md) for the wire format, then check existing frontends (`shatter-ts/`, `shatter-go/`) for reference implementations.
- **Designing framework-specific execution support?** Read [docs/execution-adapters.md](execution-adapters.md) for the long-term execution-adapter architecture, then use [SPEC.md](../SPEC.md) and [PROTOCOL.md](../PROTOCOL.md) to separate implemented behavior from planned subsystem design.
- **Contributing code?** Start with [CONTRIBUTING.md](../CONTRIBUTING.md), then read [CLAUDE.md](../CLAUDE.md) for quality standards and [AGENTS.md](../AGENTS.md) for workflow. Each sub-crate has its own `CLAUDE.md` with component-specific guidance.
- **Understanding the vision?** [PLAN.md](../PLAN.md) describes the v2 architecture and roadmap — but check [SPEC.md](../SPEC.md) to know what's actually implemented today.

## Roadmap vs Reality

[PLAN.md](../PLAN.md) describes the target architecture and features that may not yet be implemented. [SPEC.md](../SPEC.md) is the authoritative reference for current behavior — the `/audit` process compares the codebase against SPEC.md, not PLAN.md. When PLAN.md and SPEC.md disagree, SPEC.md reflects reality.
