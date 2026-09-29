#!/usr/bin/env python3
"""Pre-commit guard: reject staged blobs containing a complete private-key block.

Inspects index blobs (not working-tree files) for line-delimited
``-----BEGIN [<TYPE> ]PRIVATE KEY-----`` ... matching END blocks with a
non-empty body (OpenSSH, RSA, EC, DSA, generic, ENCRYPTED, ...), regardless of
file extension or ignore rules. Emits only the path and key type, never any
matching line or key bytes. Any git/blob read failure exits nonzero (fail
closed).

This is format-based prevention, not proof of key validity or an exhaustive
secret scanner.

Exit codes: 0 clean, 1 private key found, 2 git/blob read error.
"""

import json
import re
import subprocess
import sys

SUBMODULE_MODE = "160000"

# Whole-line delimiters only, so prose that merely mentions the marker passes.
BEGIN_RE = re.compile(
    rb"^[ \t]*-----BEGIN ((?:[A-Z0-9]+ )*)PRIVATE KEY-----[ \t]*\r?$", re.M
)


class GitError(Exception):
    pass


def staged_blobs():
    """Yield (path_bytes, blob_sha) for added/changed staged entries."""
    proc = subprocess.run(
        ["git", "diff", "--cached", "--raw", "-z", "--no-renames",
         "--no-abbrev", "--diff-filter=ACMRT"],
        capture_output=True,
    )
    if proc.returncode != 0:
        raise GitError("git diff --cached failed")
    tokens = proc.stdout.split(b"\0")
    if tokens and tokens[-1] == b"":
        tokens.pop()
    if len(tokens) % 2 != 0:
        raise GitError("unparseable git diff --cached output")
    for meta, path in zip(tokens[0::2], tokens[1::2]):
        fields = meta.lstrip(b":").split()
        if len(fields) < 5:
            raise GitError("unparseable git diff --cached record")
        if fields[1].decode() == SUBMODULE_MODE:
            continue
        yield path, fields[3].decode()


def read_blobs(shas):
    """Return {sha: bytes} via one `git cat-file --batch` process."""
    proc = subprocess.Popen(
        ["git", "cat-file", "--batch"],
        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
    )
    blobs = {}
    try:
        for sha in shas:
            proc.stdin.write(sha.encode() + b"\n")
            proc.stdin.flush()
            header = proc.stdout.readline().split()
            if len(header) != 3 or header[1] != b"blob":
                raise GitError("cannot read staged blob")
            size = int(header[2])
            data = proc.stdout.read(size)
            if len(data) != size or proc.stdout.read(1) != b"\n":
                raise GitError("short read of staged blob")
            blobs[sha] = data
    finally:
        proc.stdin.close()
        proc.stdout.close()
        proc.wait()
    return blobs


def find_key_type(data):
    """Return the key type of the first complete private-key block, or None."""
    for begin in BEGIN_RE.finditer(data):
        label = begin.group(1).decode()
        end_re = re.compile(
            rb"^[ \t]*-----END " + re.escape(label.encode()) +
            rb"PRIVATE KEY-----[ \t]*\r?$", re.M)
        end = end_re.search(data, begin.end())
        if end and data[begin.end():end.start()].strip():
            return (label + "PRIVATE KEY")
    return None


def display_path(path):
    # JSON-escape so newlines/control bytes cannot forge diagnostic lines.
    return json.dumps(path.decode("utf-8", "backslashreplace"))


def main():
    try:
        entries = list(staged_blobs())
        blobs = read_blobs(sorted({sha for _, sha in entries}))
    except (GitError, OSError, ValueError) as exc:
        print(f"[shatter] private-key guard error (failing closed): {exc}",
              file=sys.stderr)
        return 2

    found = 0
    for path, sha in entries:
        key_type = find_key_type(blobs[sha])
        if key_type:
            found += 1
            print(f"[shatter] staged private key block ({key_type}) in "
                  f"{display_path(path)}", file=sys.stderr)
    if found:
        print("[shatter] commit rejected: unstage the file(s) "
              "(git rm --cached) and remove the key material.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
