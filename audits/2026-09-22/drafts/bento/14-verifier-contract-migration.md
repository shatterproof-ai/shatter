# Verifier contract drift: warn when verifier payloads lack `executed`, require gate output pass-through

- Filing action: new issue
- Priority: P2
- Type: feature
- Labels: audit, land-work
- Parent: epic
- Links: related bento-rdtn.6, related bento-rdtn.4
- Source findings: bento-13, agent-repo-11 (context)

---BODY---
## Problem

bento-rdtn.6 added a per-check `executed` flag and an all-cached guard. By design, payloads that do not include the field are accepted silently. Verifiers wired before the contract changed therefore get no protection and no signal. Shatter's hand-written verifier:

- emits only `{name, status}`;
- runs every gate with `>/dev/null 2>&1`, so even the rdtn.4 persisted log is empty.

Shatter's `task check`/`test-standard` also reports cached "is up to date" leaves as passes. A cached no-op landing gate therefore looks identical to a real run.

## Evidence

- Shatter `scripts/land_work_verifier.sh` (last changed d01a22db, 2026-08-05) is as described above.
- None of the 11 observed land.py verify lines in shatter show `[cached]` or `[executed]`.

## Current code facts (bento @ 1c0c1e6)

- `catalog/skills/land-work/scripts/land-work-run-verifier.py` about lines 522-541: the guard trips only when every check has `executed is False`. A missing field passes.
- `land.py` lines 116-121 print `cached` only when the flags exist.
- `catalog/skills/wire-land-verifier/scripts/wire-land-verifier.py` about lines 853-917 generates wrappers that include `executed`, but existing repos are never prompted to regenerate.

## Acceptance criteria

- When any check in the payload lacks `executed`, run-verifier adds a warning, `checks without execution evidence: <names>`, that land.py prints on the verify step line.
- The doctor flags a verifier.json or wrapper that predates the current contract version (add `contract_version` to verifier.json), with the text "regenerate with wire-land-verifier".
- The wire-land-verifier reference doc states that wrappers must pass gate stdout/stderr through, not to /dev/null.
- Tests cover the new warning.

## Out of scope

- Rewriting shatter's verifier. That is shatter str-qwua7.55 / str-qwua7.2.
