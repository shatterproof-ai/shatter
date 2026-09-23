# Cross-check review

- **Reviewer:** codex (independent runtime)
- **Artifact type:** issue
- **Mode:** cross
- **Artifact SHA-256:** 270e24288b3defd47a19f280257d36486fba515e57b0ffc11da88b5878ef5c5e
- **Scope:** 2026-09-22 Shatter audit issue drafts, bucket shatter-docs (shatter). Repos to verify claims against (read by absolute path): shatter at /home/ketan/.local/share/worktrees/shatter/audit-2026-09-22 (audit worktree == main 16794cef + audit files), bento /home/ketan/project/bento, bugshot /home/ketan/project/bugshot, storystore /home/ketan/project/storystore, shatter-agents /home/ketan/project/shatter-agents, dotfiles /home/ketan/dotfiles. Tracker snapshots: <repo>/.beads/issues.jsonl (bd may be unusable read-only).


## Findings

Not ready to file as-is. I checked source at `56c86168` and committed tracker records; live tracker status was not revalidated.

1. **MAJOR — #02/#03 have incompatible artifact-table requirements.** #03 requires every row to have a default path under `shatter-artifacts/` and a schema supplied by #02, but staged commands use caller-selected output paths, #02 does not require staged-output schemas, and Markdown/HTML/text outputs have no JSON Schema. Reconcile the inventories and explicitly permit “stdout,” “caller-selected path,” and “not applicable.”

2. **MAJOR — #06’s document-list acceptance cannot pass.** The required equality to INDEX documents whose audience includes users excludes required additions: `execution-adapters.md` targets contributors/architects, and `CI-INTEGRATION.md` targets contributors; the already-covered INDEX does not list itself. Require coverage of the user-document set as a subset, with explicit additional documents.

3. **MAJOR — #06 leaves schema validation vulnerable to cached passes.** `Taskfile.yml`’s docs-smoke inputs omit protocol schemas, and the draft only requires adding document inputs. Once validation depends on schemas, schema-only changes must invalidate the task cache too.

4. **MAJOR — #09’s reverse flag check would reject valid documentation.** `SPEC.md:176` correctly mentions Node’s backticked `--max-old-space-size`, which is not a Shatter clap flag. The reverse check needs command-aware context or an explicit allowance for external-tool flags; its requirement to report “exactly” the listed drift is therefore unsound.

5. **MAJOR — #09’s changelog rule permits a header-only bypass.** As written, failure requires both no new row and an unchanged header, so changing only the header satisfies the gate. Define separate requirements for a new row and a date consistent with that row, including same-day changes and explicit exemptions.

6. **MAJOR — #05 duplicates deliverables already assigned elsewhere.** The committed tracker acceptance for `str-qwua7.25` already owns frontend line-number replacement and both `.js` corrections; `str-qwua7.24` owns removing the TS-only `ite` claim. Excluding slimming and table generation does not transfer those deliverables—update the existing acceptance or remove the overlap.

7. **MAJOR — #07’s proposed dependency-graph validator misses actual execution paths.** `workspace-test` is invoked through `cmds`, Full runs through a shell wrapper and sequential commands, and Affected selects gates dynamically. Specify traversal of those paths and how conditional coverage is represented, or restrict validation to fixed tiers.

8. **MAJOR — #08/#10 overstate the clap extractor dependency.** The recorded `str-u394l.3` requirements cover story content, evidence, index validity, and patrol wiring; `check_docs_stories` already checks an existing index without CLI extraction. Distinguish blocked automatic CLI completeness analysis from the basic stories gate that can proceed now.

9. **MINOR — #06 specifies the wrong discriminator for response examples.** Protocol responses use `status`, not `command`, so the requested request/response schema selection cannot work as described. Specify explicit schema tags or separate request and response detection.

10. **MINOR — #10 leaves expiry and extension semantics ambiguous.** “`created_at`, or … `pending_since`” does not define whether a committed extension overrides issue age or what happens when tracker data is unavailable. Specify timestamp precedence, the exact 60-day boundary, and unavailable-data behavior.

The top fixes are to reconcile the schema and documentation contracts, make the proposed gates’ pass/fail conditions executable and internally consistent, and explicitly resolve duplicate ownership and the overstated stories blocker.
