# Project Layout

This document describes the directories and configuration files Shatter creates
or uses inside a project, plus a few tool-owned cache locations outside the
project tree.

Use this as the reference for:

- what you are expected to edit
- what Shatter may create automatically
- what is cache vs artifact vs configuration
- which paths are current and which are legacy compatibility paths

## Quick Reference

| Path | Purpose | Edit by hand? | Typical lifecycle |
|------|---------|---------------|-------------------|
| `shatter.config.json` | Scan-global defaults (discovery, output, cache, resource limits) | Yes | Durable |
| `.shatter/` | Project-local Shatter configuration and some shared state | Yes, selectively | Kept in the project |
| `.shatter/config.yaml` | Hierarchical per-function config (iterations, timeouts, mocks, generators) | Yes | Durable |
| `shatter.scope.yaml` | Scope/include/exclude and mocking controls | Yes | Durable |
| `.shatter/setup.<ext>` | Session-level setup file | Yes | Durable |
| `shatter.setup.<ext>` | Preferred root-level session setup file | Yes | Durable |
| `<file>.shatter.setup.<ext>` | File-level setup file next to a source file | Yes | Durable |
| `.shatter/seeds/pool.json` | Cross-function seed pool | Usually no | Generated, reusable |
| `.shatter/test-markers/` | Test impact analysis data | Usually no | Generated |
| `.shatter/cache/harness/` | Reusable harness/build cache | No | Generated cache |
| `.shatter-cache/analysis/` | Analysis cache | No | Generated cache |
| `.shatter-cache/behavior-maps/` | Behavior/spec cache | No | Generated cache |
| `.shatter-cache/bin/` | Custom built frontend binaries | Usually no | Generated cache |
| `shatter-artifacts/` | Preserved user-visible artifacts | Usually no | Durable generated output |
| `shatter-artifacts/recorded-mocks/` | Recorded mock fixtures from `--record` | Usually no | Durable generated output |
| `$XDG_CACHE_HOME/shatter/` or `~/.cache/shatter/` | Embedded frontend bundle cache | No | Tool-owned cache outside project |
| `$TMPDIR/shatter-scratch-*` | Per-run scratch space | No | Ephemeral |

## `.shatter/`

`.shatter/` is the main project-local Shatter directory.

`shatter init` creates it, and some commands will initialize it implicitly if it
does not already exist. Today, `init` guarantees creation of:

- `.shatter/`
- `.shatter/config.yaml`

Primary role:

- user-edited configuration
- setup files
- some project-local generated state
- some project-local caches used by the harness and test-impact features

## `shatter.config.json`

Optional project-root file for **scan-global** defaults: file discovery,
output format, caching, and resource limits. These settings apply uniformly to
an entire scan run and do not vary per-function.

Fields: `include`, `exclude`, `language`, `max_depth`, `timeout_total`,
`exec_timeout`, `parallelism`, `output`, `cache_dir`, `no_cache`, `seeds_dir`,
`capture_side_effects`. See the README for the full schema reference.

Per-function settings (iterations, timeouts, mocks, genetic algorithm,
generators, setup) do **not** belong here — use `.shatter/config.yaml` instead.

## `.shatter/config.yaml`

Hierarchical per-function configuration file for Shatter.

What it is used for:

- default exploration settings (iterations, timeouts)
- per-function overrides
- candidate input files
- setup configuration
- opaque type declarations
- mock fixture overrides
- nondeterminism review decisions
- generator configuration for custom frontends
- genetic algorithm settings

Important behavior:

- Shatter supports hierarchical config discovery.
- You can place `.shatter/config.yaml` at multiple levels in a project tree.
- When multiple configs apply, the nearest config to the target file wins on conflicts.

Paths inside this config are commonly resolved relative to the `.shatter/`
directory that contains the config.

## `shatter.scope.yaml`

`shatter.scope.yaml` is a separate scope file, not part of `.shatter/`.

Use it for:

- include/exclude rules
- scoping which files or functions are considered
- controlling which dependencies are mocked

This file is user-authored and should be treated as durable project config.

## Setup Files

Shatter supports convention-based setup files.

Session-level setup:

- `shatter.setup.<ext>` at the project root
- `.shatter/setup.<ext>` as an alternate location

If both exist, the root-level `shatter.setup.<ext>` takes precedence.

File-level setup:

- `<stem>.shatter.setup.<ext>` next to a source file

Examples:

- `shatter.setup.ts`
- `.shatter/setup.go`
- `auth.shatter.setup.ts`

These are user-authored files.

## `.shatter/seeds/pool.json`

This file stores the cross-function seed pool.

Purpose:

- preserve interesting discovered values
- reuse them as candidate inputs for other functions with compatible parameter types

This is generated state. You normally would not edit it by hand.

You may want to delete it if you want to reset accumulated seeds.

## `.shatter/test-markers/`

This directory is used by Shatter's test impact analysis.

Current generated files include:

- `.shatter/test-markers/coverage-map.yaml`
- `.shatter/test-markers/tiers/`

Purpose:

- remember which tests touch which files
- record passing tier markers

This is generated state, not user-authored config.

## `.shatter/cache/harness/`

This directory is the reusable harness/build cache for the semantic harness
system.

Purpose:

- compiled harnesses
- instrumented sources
- reusable build outputs that should stay project-scoped

This is cache, not durable user-facing output.

You should not edit it by hand.

## `.shatter-cache/`

`.shatter-cache/` is a separate project-local cache root used by several
features.

Current paths documented in code:

- `.shatter-cache/analysis/`
- `.shatter-cache/behavior-maps/`
- `.shatter-cache/bin/`

### `.shatter-cache/analysis/`

Stores analysis cache entries.

Purpose:

- avoid repeating analysis work for unchanged files

### `.shatter-cache/behavior-maps/`

Stores cached behavior maps and related spec cache data.

Purpose:

- incremental reuse across runs
- behavior-map-backed workflows such as scan/review/revalidation

This is the default location behind `--cache-dir`.

### `.shatter-cache/bin/`

Stores custom frontend binaries built by `shatter build-frontend`.

Current convention:

- `.shatter-cache/bin/shatter-go-custom`
- `.shatter-cache/bin/shatter-rust-custom`

Legacy compatibility still checks `.shatter/bin/`, but `.shatter-cache/bin/` is
the current location.

## `shatter-artifacts/`

`shatter-artifacts/` is the project-root location for preserved generated
artifacts.

Think of this directory as:

- user-visible
- durable
- worth inspecting or keeping

In contrast, caches under `.shatter/` or `.shatter-cache/` are primarily for
reuse and performance.

The most clearly documented current artifact path in code is:

- `shatter-artifacts/recorded-mocks/`

The broader design direction in the codebase also treats `shatter-artifacts/`
as the home for preserved reports, exports, and traces.

## `shatter-artifacts/recorded-mocks/`

This directory stores recorded mock fixtures generated by record mode.

Created by flows using:

- `shatter explore --record`

Purpose:

- capture observed external dependency behavior
- seed future runs with recorded fixtures

Legacy note:

- older code/comments refer to `.shatter/recorded-mocks/`
- current code treats that as a legacy location and uses `shatter-artifacts/recorded-mocks/`

## Tool-Owned Cache Outside The Project

Not all Shatter files live inside your repository.

### Embedded frontend bundle cache

The CLI extracts the embedded TypeScript frontend bundle into:

- `$XDG_CACHE_HOME/shatter/` if `XDG_CACHE_HOME` is set
- otherwise `~/.cache/shatter/`

This cache is outside the project tree and should not be committed or edited.

### Scratch directories

The harness system also uses temp-scoped scratch directories under the system
temp directory, for example:

- `$TMPDIR/shatter-scratch-<id>/`

These are ephemeral working directories for a run, not durable project files.

## Other Project-Local Files You May See

### `.shatter/crypto-registry.toml`

This optional file extends the built-in crypto registry.

Use it when you want to teach Shatter about additional cryptographic APIs in
your codebase.

### Inputs files referenced from config

`.shatter/config.yaml` can reference candidate input JSON files. Those files are
user-authored project inputs, even though the exact path is up to you.

## Current vs Legacy Paths

The storage model is still in transition in a few places. The safest current
mental model is:

- user config and setup live under `.shatter/` or in named root-level setup files
- project-local caches live under `.shatter/` and `.shatter-cache/`
- preserved artifacts should live under `shatter-artifacts/`
- tool-owned machine cache may live under `$XDG_CACHE_HOME/shatter/` or `~/.cache/shatter/`

Legacy paths still referenced in code or compatibility logic:

- `.shatter/bin/` for custom frontends
- `.shatter/recorded-mocks/` for recorded fixtures

If you are creating new project documentation or examples, prefer the current
paths:

- `.shatter/config.yaml`
- `shatter.scope.yaml`
- `.shatter-cache/`
- `shatter-artifacts/`

## What To Commit

Usually commit:

- `.shatter/config.yaml`
- `shatter.scope.yaml`
- setup files such as `shatter.setup.ts`
- optional `.shatter/crypto-registry.toml`

Usually do not commit:

- `.shatter-cache/`
- `.shatter/cache/harness/`
- `.shatter/seeds/pool.json`
- `.shatter/test-markers/`
- `shatter-artifacts/` unless you explicitly want generated artifacts versioned
- `$XDG_CACHE_HOME/shatter/` or `~/.cache/shatter/`

Your exact policy may differ, but that split matches the intended distinction
between user-authored config and generated state.
