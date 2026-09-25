---
slug: diff-name-freed-note
kind: note-to-existing
title: "Note on open epic str-81xiw: snapshot `shatter diff` is being retired, so the `diff` subcommand name becomes free (informational)"
priority: P2
type: note
labels: [audit-2026-09-22]
parent_epic: ""
blocked_by: [retire-snapshot-diff]
existing_id: str-81xiw
tracker: "bd in /home/ketan/project/shatter (prefix str)"
---

# Note on open epic str-81xiw: snapshot `shatter diff` is being retired, so the `diff` subcommand name becomes free (informational)

## Tracker action

- Add the comment below to **str-81xiw** (`bd comments add str-81xiw …`).
- Change nothing else: not the priority, the scope, the children (str-81xiw.2/.3/.4) or the planned command name.
- Filing order: file retire-snapshot-diff first, then replace `<retire-snapshot-diff id>` with its real id.

## Comment text

> Informational, from audit 2026-09-22 and maintainer decision D2 (2026-09-23).
>
> This epic's design notes say `shatter-cli/src/args.rs` "already defines `shatter diff <snapshot> <current>` for snapshot comparison. Do not repurpose that command incompatibly", and they use `shatter diff-explore` as the v1 name. That constraint is going away. Under D2 the snapshot `shatter diff` command and its Snapshot module are being removed, and `shatter spec-diff` becomes the regression tool, in **<retire-snapshot-diff id>**. That issue adds no alias or shim, so once it lands the `diff` subcommand name is unused.
>
> Whether diff-scoped exploration takes the `diff` name or keeps `diff-explore` is for this epic to decide. D2 and the retirement issue deliberately do not decide it. Nothing in this epic's scope changes. If the epic does choose `diff`, note that the shatter-agents plugin's `shatter-diff` skill currently documents a nonexistent `shatter diff --staged` (being withdrawn in shatter-agents withdraw-shatter-diff-skill). Any future plugin guidance should follow whatever name this epic picks.
