# Kapow Refute Agent Workflow

Agents running Kapow validation can use Refute for symbol-aware refactors
without rediscovering the local checkout or install path.

## Wrapper

From the Shatter repo:

```bash
python3 scripts/kapow_refute_agent.py check
python3 scripts/kapow_refute_agent.py smoke
```

The wrapper defaults to:

| Path | Purpose |
| --- | --- |
| `~/project/kapow` | Kapow project root |
| `~/project/refute` | Refute source checkout |
| `~/project/kapow/.agents/bin/refute` | Project-local Refute binary |

Use `--project` or `--refute-checkout` when validating a different checkout:

```bash
python3 scripts/kapow_refute_agent.py --project /path/to/kapow check
```

## Install Or Update

If `check` reports `missing_refute`, install or update the project-local binary:

```bash
python3 scripts/kapow_refute_agent.py install
```

That delegates to:

```bash
bash ~/project/refute/scripts/install-nightly.sh --project ~/project/kapow
```

The binary is intentionally installed under Kapow's untracked `.agents/bin/`
directory. Do not commit the binary into Kapow or Shatter.

## Smoke Check

Run:

```bash
python3 scripts/kapow_refute_agent.py smoke
```

The smoke check proves:

- `~/project/kapow/.agents/bin/refute` exists and is executable.
- `refute version` runs from the Kapow root.
- `refute doctor` reports the available language backends.

Use `--json` when another agent or script needs machine-readable status:

```bash
python3 scripts/kapow_refute_agent.py --json check
python3 scripts/kapow_refute_agent.py --json doctor
```

## Agent Use

Default to dry-run JSON previews before applying any refactor:

```bash
~/project/kapow/.agents/bin/refute rename --dry-run --json \
  --file ~/project/kapow/api/path/to/file.go \
  --line <line> \
  --name <oldName> \
  --new-name <newName>
```

If the preview is correct, apply the same command without `--dry-run`, then run
Kapow's required verification gate for the changed files.

## Failure Modes

| Status | Meaning | Action |
| --- | --- | --- |
| `missing_project` | The Kapow checkout path does not exist. | Pass `--project` with the correct checkout path. |
| `missing_refute` | The project-local Refute binary is absent. | Run `python3 scripts/kapow_refute_agent.py install`. |
| `not_executable` | The Refute file exists but cannot be executed. | Re-run `install`, or fix the local file mode. |
| non-zero `doctor` | A language backend such as `gopls` is unavailable. | Install the backend shown by Refute before refactoring that language. |

Refute v0.1 is a single-shot CLI. Go/gopls is the supported path; Rust and
TypeScript/JavaScript are experimental, Python is planned, and Java/Kotlin are
not claimed for this release.
