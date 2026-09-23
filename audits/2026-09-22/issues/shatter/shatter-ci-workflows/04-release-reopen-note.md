---
slug: release-reopen-note
kind: reopen-note
title: "Comment on closed str-lj7s: closed on 'landed' with no green run; release.yml has 0 successes in 267 runs"
priority: P1
type: note
labels: [release, ci, audit]
parent_epic: "Epic: Audit 2026-09-22 findings"
blocked_by: []
existing_id: str-lj7s
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Comment on closed str-lj7s

Target: **str-lj7s** (closed). Post as a comment only. Do not reopen.

Comment text:

> Audit 2026-09-22 follow-up. This issue was closed on "landed on main" (ab2a8515) with no green `release.yml` run cited. The workflow has never succeeded since: as of 2026-09-23, `gh run list --workflow release.yml -L 300` shows 169 failures, 98 cancellations and 0 successes, and `gh release list` is empty. As a result, `install.sh` and the `action.yml` GitHub Action cannot install Shatter for anyone.
>
> Two matrix legs fail on every run (latest: run 35773969737):
> - `x86_64-pc-windows-msvc`: `z3-sys v0.10.7 ... wrapper.h:1:10: fatal error: 'z3.h' file not found`
> - `aarch64-unknown-linux-gnu` (cross): `failed to run custom build command for openssl-sys v0.9.116`
>
> The release job `needs` every leg, so it is skipped every time.
>
> Maintainer decision D1 (2026-09-23): both targets stay in the matrix and get fixed. The work is tracked in four new issues:
> - `<id of release-publish-guard-and-target>`: restrict publishing to push-on-main and pass `--target $GITHUB_SHA`, so branch runs can iterate on the fixes without publishing
> - `<id of release-windows-z3-build>`: the Windows Z3 build, plus the Unix-only code in shatter-cli that fails next
> - `<id of release-aarch64-openssl-cross>`: the aarch64 openssl/cross build, plus build.rs embedding a host-arch Go frontend (str-qwua7.41 deleted Cross.toml and cross/ on the false premise that no workflow uses cross)
> - `<id of release-publish-and-install-smoke>`: the first published continuous-* prerelease, and install.sh/action.yml smoke tests on clean runners inside release.yml
>
> Each closes only with a green release-run URL. Release work should not be closed on "landed" again.

(Filer: replace the `<id of ...>` placeholders with the ids assigned to those slugs.)
