# Build and Release workflow has 0 successes in 200 runs (Windows z3.h, aarch64 openssl-sys); no release exists so install.sh/action.yml cannot install

| field | value |
|---|---|
| action | new issue (child of audit epic) |
| type | bug |
| priority | P1 |
| labels | release,ci,distribution,audit |
| parent | audit epic (draft 00) |
| blocked by | none |
| related | str-lj7s, str-qwua7.41, str-74j.1 |
| source findings | tests-ci-02, prior-02 |

<!-- body -->
## Problem

release.yml has never produced a release. install.sh needs a `continuous-*` prerelease, so the documented installer and GitHub Action always fail. str-lj7s was closed on 'landed' with no green run; str-qwua7.41 then deleted Cross.toml/cross/ on the false premise that no workflow uses cross.

## Current code facts / evidence

- `gh run list --workflow release.yml -L 200` → failure 168, cancelled 32, success 0.
- Run 35756993200: x86_64-pc-windows-msvc `wrapper.h:1:10: fatal error: 'z3.h' file not found` (z3-sys 0.10.7); aarch64-unknown-linux-gnu cross build 'failed to run custom build command for openssl-sys v0.9.116'; other targets succeed; 'Create continuous GitHub Release' skipped.
- `gh release list` empty; `install.sh:67-81` requires a continuous-* prerelease; `action.yml:47` calls install.sh.
- release.yml:59, 121-160 use cross.

## Acceptance criteria

- Windows builds (bundled/static Z3 or vcpkg) or is dropped from the matrix with a documented reason.
- aarch64 builds (rustls or vendored openssl, restoring cross config if needed) or is dropped.
- Release job publishes the targets that did build rather than skipping entirely.
- A green release run exists and install.sh smoke test against it passes (CI step).

## Suggested approach

Unblock publishing first (publish partial matrix), then fix targets.

## Scope

- In scope: the acceptance criteria above.
- Out of scope: unrelated refactors in the touched files.
- Size: M

## References

- Audit findings: tests-ci-02, prior-02 (audit 2026-09-22; evidence under `audits/2026-09-22/`).
- Related issues: str-lj7s, str-qwua7.41, str-74j.1
