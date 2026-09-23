---
slug: ubuntu-26-runner-trial
kind: new
title: "Trial the CI and release workflows on Ubuntu 26 runners and lift the ubuntu-24.04 pin once they pass"
priority: P3
type: chore
labels: [ci, github-actions, maintenance, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: [workflow-action-versions]
existing_id: ""
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Trial the CI and release workflows on Ubuntu 26 runners and lift the ubuntu-24.04 pin once they pass

## Problem

`workflow-action-versions` pins every Linux runner to `ubuntu-24.04`, because `ubuntu-latest` starts migrating to Ubuntu 26 on 2026-10-19 and the workflows depend on distro packages (`libclang-dev`, `z3`) that have not been verified there. A pin with no follow-up would last forever, and the runner image would eventually be retired from under it. This issue does the trial and removes the pin.

It was pre-drafted so that the maintainer's filer creates it (D6), instead of `workflow-action-versions` asking its implementer to file it.

## Evidence

- CI run 35756993223 has the annotation "The ubuntu-latest label will migrate to Ubuntu 26 beginning October 19, 2026".
- `ci.yml:61-64` runs `apt-get install libclang-dev z3`. `release.yml:115-119` installs `libclang-dev`. `devcontainer.yml` builds its own image. `perf-ci.yml` and `drift-patrol.yml` set up toolchains through actions.

## Acceptance criteria

- [ ] Each workflow that `workflow-action-versions` pinned runs once on an Ubuntu 26 label (`ubuntu-26.04`, once GitHub offers it), via `workflow_dispatch` from a branch or a temporary matrix entry. Record the run URLs.
- [ ] Any package or toolchain breakage is fixed, for example renamed `libclang` or `z3` packages, or a different default Python.
- [ ] The pins are changed to `ubuntu-26.04`, or back to `ubuntu-latest`, and the pin comments that name this issue are removed.
- [ ] **Close-time proof.** The close reason contains green run URLs on Ubuntu 26 for `ci.yml` and for the Linux legs of `release.yml`.

## Out of scope

- Action major-version bumps (`workflow-action-versions`).
- macOS and Windows runner labels.

## Dependencies

- Blocked by: `workflow-action-versions`.

Priority: P3 · Type: chore · Labels: ci, github-actions, maintenance, audit · Parent: Epic: Audit 2026-09-22 findings · Sources: shatter-code/77, tests-ci-18 (split from workflow-action-versions in revision) · Decision: D6
