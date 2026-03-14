# Plan: str-k3mj — AGENTS.md beads workflow refresh

## Context
AGENTS.md references `bd sync` (3 occurrences) and `docs/QUICKSTART.md` (1 occurrence), neither of which exist. `bd sync` is not a valid bd CLI command, and `docs/QUICKSTART.md` is not present in the repo.

## Changes to AGENTS.md

### 1. Remove `bd sync` from Quick Reference (line 12)
Delete `bd sync               # Sync with git` from the quick reference block.

### 2. Fix Landing the Plane push sequence (lines 43-46)
Replace:
```bash
git pull --rebase
bd sync
git push
```
With:
```bash
git pull --rebase
git push
```

### 3. Fix Completing an Issue (line 156)
Replace `bd sync && git push` with just `git push`.

### 4. Update Auto-Sync section (lines 396-401)
Remove the claim about a `bd sync` command. The auto-sync section describes automatic JSONL export/import behavior — reword to clarify this is built-in, not a command.

### 5. Remove `docs/QUICKSTART.md` reference (line 412)
Replace `For more details, see README.md and docs/QUICKSTART.md.` with `For more details, see README.md.`

## Files modified
- `AGENTS.md` (5 edits, all in one file)

## Verification
- Grep AGENTS.md for `bd sync` — should return 0 matches
- Grep AGENTS.md for `QUICKSTART` — should return 0 matches
- All remaining `bd` commands in AGENTS.md should be valid per `bd --help`
