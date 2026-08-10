# Drift Patrol

The drift patrol is Shatter's recurring governance sweep. It runs a small,
bounded set of drift checks on a schedule so that documentation, protocol
metadata, parity contracts, and tracker state cannot silently rot between
manual audits.

**Entry point:**

```bash
task drift-patrol                  # builds the frontends first, then patrols
python3 scripts/drift-patrol.py    # same checks, degrades gracefully
```

Exit code `0` means every enabled check passed; `1` means at least one check
found drift.

## Cadence and audience

| | |
|---|---|
| **Trigger** | `.github/workflows/drift-patrol.yml` — weekly, Mondays 09:00 UTC, plus manual `workflow_dispatch` |
| **Audience** | Repository maintainers. A failure shows up as a red scheduled run in the Actions tab and as a report in the run summary |
| **Owner** | The maintainer on the weekly triage rotation; if there is no rotation, whoever is landing work that week |
| **Scope** | Bounded checks only — the patrol is not a replacement for `/audit` or for `task check` |

The workflow also runs the patrol's own unit tests on any pull request that
touches `scripts/drift-patrol.py`, so the patrol cannot rot in place. The
patrol itself does **not** gate pull requests: it reports repository-wide
drift, and failing an unrelated branch because someone else left an issue
`in_progress` would just train people to ignore it.

## What it checks

| Check | What it catches | Owning issue |
|---|---|---|
| `protocol-registry` | `protocol/registry.yaml` drifting from the core/frontend sources | — |
| `protocol-codegen` | Generated protocol bindings out of sync with the registry | — |
| `protocol-conformance` | A frontend diverging from the protocol contract | — |
| `parity-expiry` | A resolved parity divergence within 14 days of its removal deadline | `str-5dx0` |
| `cli-surface-drift` | CLI commands missing from SPEC.md or gauntlet coverage | `str-wurp` (not implemented) |
| `docs-stories` | Missing `docs/stories`, or an `INDEX.md` older than the stories it lists | `str-u394l.3` (not implemented) |
| `tracker-hygiene` | `in_progress` issues untouched for >14 days; open children under a closed parent | — |

`protocol-conformance` is the patrol's documented fast subset of the wider
quality suite: it runs `protocol/conformance/conformance_harness.py` only, not
the language test suites. The harness silently skips frontends whose build
artifacts are missing, so the patrol detects that itself and reports `SKIP`
rather than a hollow pass. `task drift-patrol` and the CI workflow build the
frontends first and pass `--require-conformance`, which turns that skip into a
failure.

Tracker data comes from `bd list --all` when the `bd` CLI is available, and
otherwise from the committed `.beads/issues.jsonl` export — which is why the
check works in CI, where `bd` is not installed. The report names its source.

## Check statuses

| Status | Meaning | Fails the patrol? |
|---|---|---|
| `PASS` | The check ran and found no drift | no |
| `FAIL` | The check ran and found drift | **yes** |
| `PENDING` | Placeholder for a check whose implementation is tracked by another issue | no by default |
| `SKIP` | The check could not run here (missing build artifact or tracker data) | no by default |

### Why `PENDING` is not a failure by default

`str-u394l.1` allowed either a real check or "a placeholder that fails with a
clear *not implemented* message". This implementation reports `PENDING`
instead, because a weekly scheduled run that is red from day one for a
non-actionable reason teaches everyone to ignore it — the exact failure mode
the patrol exists to fix. The gap stays loud: every `PENDING` check is printed
on every run, in the summary table and in the findings section, with its
tracking issue.

Two escape hatches preserve the stricter behavior:

- `--strict-pending` promotes every `PENDING` to `FAIL`. The workflow exposes
  it as a `workflow_dispatch` input.
- A `PENDING` check whose tracking issue has been **closed** is always a
  `FAIL`, regardless of the flag. That means the work landed without wiring
  the patrol up to it — which is itself drift, and is the case worth being
  red about.

## What to do with a failure

1. **Read the report.** It is written to be pasted into a Beads issue verbatim:
   each finding carries its status, the responsible issue, the offending items,
   and a remediation command. You should not need to re-run the audit to file
   the issue.
2. **Fix it directly if it is small.** Most `protocol-registry`,
   `protocol-codegen`, and `parity-expiry` failures are a regenerate-and-commit
   or a delete-the-expired-entry away; the remediation line says which.
3. **Otherwise file or update a tracking issue** under the drift-enforcement
   epic:

   ```bash
   bd create "Drift patrol findings YYYY-MM-DD" --parent str-u394l --label drift --priority 2
   ```

   Paste the report body in. If an open issue already covers the finding,
   update that one instead of filing a duplicate.
4. **Do not silence a check by deleting it.** If a check is genuinely wrong,
   change the threshold (`--stale-days`, `--warn-within-days`) or record the
   exception in the relevant contract file (`protocol/parity-matrix.yaml` for
   parity divergences) so the exception is itself reviewable.

## Useful flags

```bash
python3 scripts/drift-patrol.py --only tracker-hygiene    # one check
python3 scripts/drift-patrol.py --skip protocol-conformance
python3 scripts/drift-patrol.py --strict-pending          # placeholders fail
python3 scripts/drift-patrol.py --stale-days 30           # looser hygiene bar
python3 scripts/drift-patrol.py --json                    # machine-readable
python3 scripts/drift-patrol.py --report drift.md         # also write to a file
```

## Extending the patrol

Add a check function to `scripts/drift-patrol.py` returning a `Result`, then
register it in `CHECKS`. Keep it bounded — the patrol's value depends on
running weekly without a maintainer deciding it is too slow to keep on. Cover
the new check in `scripts/test_drift_patrol.py`; `task meta` runs those tests,
so `task check` catches a broken patrol.

When `str-wurp` (CLI-surface drift) or `str-u394l.3` (stories coverage) lands,
replace the corresponding `pending_check` call with the real check. The patrol
will tell you: closing those issues while the placeholder is still in place
turns the run red.

## Related

- [`.github/workflows/parity-expiry.yml`](../.github/workflows/parity-expiry.yml) — the narrower, pre-existing scheduled parity-expiry watch (`str-5dx0`)
- [`protocol/GOVERNANCE.md`](../protocol/GOVERNANCE.md) — checklist for protocol changes
- [`protocol/PARITY.md`](../protocol/PARITY.md) — parity divergence register
- [`docs/CI-INTEGRATION.md`](CI-INTEGRATION.md) — the rest of the CI surface
- `/audit` skill — the deeper, manual, periodic audit the patrol does not replace
