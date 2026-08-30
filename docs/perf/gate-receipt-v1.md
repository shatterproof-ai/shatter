# Gate Receipt v1

A gate receipt binds successful quality-gate results to one exact candidate
Git tree, its base tree, the gate implementation bytes, lockfiles, and tool
versions. Receipts are local evidence only; they are not signatures.

## Command

```text
python3 scripts/gate-receipt.py write \
  --candidate TREE --base TREE --tier local|ci \
  --gate-result RESULT.json [--gate-result RESULT.json ...]
```

Success writes one receipt, prints
`{"status":"written","path":"...","digest":"sha256:..."}`, and exits 0.
Invalid arguments, trees, or gate results exit 64. Git, tool-discovery, and
filesystem failures exit 74. The writer never invokes a gate.

`TREE` is exactly 40 lowercase hexadecimal characters, exists in the local
repository, and is already a tree object: it must equal
`git rev-parse TREE^{tree}`. Both candidate and base obey this rule.

## Gate-result input

Each input file is one JSON object with exactly these fields:

```json
{"gate":"task check","argv":["task","check"],"started_at":"2026-08-30T12:00:00Z","ended_at":"2026-08-30T12:01:00Z","exit_code":0}
```

`gate` is a nonempty string, `argv` is a nonempty array of strings, timestamps
are valid RFC 3339 UTC values at whole-second precision, `started_at` is not
later than `ended_at`, and `exit_code` is the integer 0. Gate names are unique.

## Receipt schema and canonical bytes

Receipt v1 is exactly:

```text
{
  schema: integer 1,
  candidate_tree: string,
  base_tree: string,
  tier: "local" | "ci",
  gate_results: [{gate, argv, started_at, ended_at, exit_code}],
  bindings: {
    code: [{path, sha256}],
    locks: [{path, present, sha256}]
  },
  tools: [{name, version}],
  started_at: string,
  ended_at: string,
  digest: string
}
```

Arrays sort as follows: gate results by `gate`, code and lock bindings by
`path`, and tools by `name`. The top-level timestamps are the minimum gate
start and maximum gate end.

Canonical JSON is UTF-8, keys sorted recursively, with no insignificant
whitespace. Its reference Python settings are
`sort_keys=True,separators=(",", ":"),ensure_ascii=False,allow_nan=False`;
non-ASCII characters are therefore emitted as UTF-8 rather than `\\u` escapes.
To compute `digest`, omit the `digest` member entirely, encode the remaining
object canonically without a final newline, and SHA-256 those bytes.
Store the result as `sha256:<64-lowercase-hex>`. The final receipt contains the
digest and exactly one trailing newline.

## Candidate-tree bindings

All bound bytes come from candidate-tree blobs, never the working tree.

Code bindings contain every tracked path whose repository-relative POSIX path
matches `*Taskfile*.yml`, plus these required paths:

- `scripts/gate-wrapper.sh`
- `scripts/gate-receipt.py`

Each `sha256` is the lowercase SHA-256 of the raw blob bytes. Required code
missing from the candidate is invalid. Git blob reads do not follow symlinks.

Lock bindings always contain these entries, including absent files:

- `Cargo.lock`
- `shatter-rust/Cargo.lock`
- `shatter-rust-runtime/Cargo.lock`
- `shatter-ts/package-lock.json`
- `shatter-go/go.sum`

A present lock has `present:true` and the raw candidate-blob hash. An absent
lock has `present:false` and `sha256:null`. A non-blob at a required code or
lock path is invalid. Git symlinks are blobs and hash their link-target bytes.
Dirty and untracked working-tree files do not affect any binding.

Tool bindings contain exactly `cargo`, `go`, `node`, `npm`, `rustc`, and
`task`. The commands are `cargo --version`, `go version`, `node --version`,
`npm --version`, `rustc --version`, and `task --version`. Their version is the
complete successful UTF-8 stdout with leading and trailing whitespace removed.
Missing tools, failed commands, undecodable output, and empty trimmed output
are discovery failures.

## Storage and atomicity

Let `common_dir` be `realpath(git rev-parse --git-common-dir)` and let
`repo_key` be the lowercase SHA-256 of that path's filesystem-encoded bytes,
without a trailing newline. The receipt
path is:

```text
${XDG_RUNTIME_DIR:-/tmp}/shatter-gate-receipts/v1/<repo_key>/<candidate-tree>.json
```

The writer owns the `shatter-gate-receipts`, `v1`, and repository-key
directories beneath the runtime root; it creates or tightens each to mode 0700
without changing the runtime root itself. Receipt files use mode 0600. Writers
create a complete temporary file in the destination directory, flush it, and
atomically replace the candidate receipt. Concurrent writers for the same
candidate may replace one another, but readers observe only a complete old or
new receipt.

## Validation

```text
python3 scripts/gate-receipt.py validate \
  --candidate TREE --base TREE --tier local|ci \
  --requirements REQUIREMENTS.json [--path RECEIPT.json]
```

The supplied candidate and base are authoritative expected local tree objects
and obey the same direct-tree validation as `write`. Without `--path`, the
validator reads the default repository/candidate path above without creating
or changing any file or directory. With `--path`, it reads that exact caller-
owned path and checks only the receipt file's mode, not its parent directories.

Requirements v1 is exactly:

```json
{"schema":1,"requirements":[{"gate":"task check","argv":["task","check"]}]}
```

`requirements` is an ordered array of unique, nonempty gate names paired with
nonempty string argument arrays. Its order does not change validation: each
named gate must occur exactly once in the receipt, have `exit_code` equal to
the integer 0, and have exactly the required `argv`. Extra receipt gates are
allowed.

The validator independently recomputes the receipt digest, candidate-tree code
and lock bindings, and current tool versions. It compares the receipt's tree,
base, tier, and required gates to the supplied policy. It never invokes a gate
and never writes. A valid receipt prints
`{"reasons":[],"status":"valid"}` and exits 0. A readable but invalid receipt
or requirements file prints `{"reasons":[...],"status":"invalid"}` and exits
65. Repository, tool-discovery, missing-file, and other I/O failures print no
JSON and exit 74. Invalid validator arguments or trees are malformed policy
input and also produce the exit-65 JSON result.

Invalid reasons are unique and always returned in this fixed order:

```text
malformed, permissions, digest, tree, base, code, lock, tool, tier,
duplicate_gate, missing_gate, failed_gate, argv
```

`malformed` covers non-UTF-8/invalid JSON, duplicate JSON keys, wrong or extra
schema fields, invalid field types or timestamps, inconsistent top-level
timestamps, noncanonical array ordering, and invalid requirements. A
structurally valid receipt may still contain duplicate gate names or nonzero
integer exit codes so those conditions can receive their specific reasons.
For every safely usable parsed field, checksum and semantic checks continue
even when another structural defect also produces `malformed`, and report every
applicable reason. The default receipt file must be a regular file owned by the
current user with mode 0600. Each writer-owned directory beneath the runtime
root must be a real directory owned by the current user with mode 0700; any
mismatch produces `permissions` without changing it.
