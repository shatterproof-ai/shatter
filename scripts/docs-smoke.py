#!/usr/bin/env python3
"""Docs examples smoke gate (str-u394l.2).

Documentation drift is not limited to a missing command inventory (that is
str-wurp's job — mechanical CLI inventory vs. clap definitions). README,
QUICKSTART, SPEC, and docs examples can also contain *stale flags*, *removed
commands*, *invalid JSON/YAML snippets*, or invocations that no longer match
the current CLI. A past audit found SPEC.md documenting removed flags such as
`explore --timeout`, scan `--output-dir`, and a global `--perf`.

This gate extracts fenced code blocks from a configured set of docs and:

  - **Shell examples** (```bash / ```sh / ```shell / ```console): every
    `shatter ...` invocation is checked against the *built CLI*. An unknown
    subcommand or an unknown flag fails the gate. This is what catches a
    reintroduced `shatter explore --timeout`.
  - **JSON examples** (```json): parsed with json.loads; invalid syntax fails.
  - **YAML examples** (```yaml / ```yml): parsed with yaml.safe_load; invalid
    syntax fails.
  - **Runnable smoke commands** (from the config's `smoke_commands`): executed
    against the built CLI in a throwaway temp directory to prove the command
    surface is actually live, not just statically consistent.

This is a *maintained allowlist*, not blind execution of every fenced block.
Intentionally illustrative snippets (pseudo-JSON, NDJSON streams, output
samples, planned-but-unimplemented commands) are marked non-runnable with an
inline directive on the line immediately above the opening fence:

    <!-- docs-smoke: skip reason="NDJSON stream, one object per line" -->
    ```json
    {"a":1}
    {"a":2}
    ```

A `skip` directive **requires** a non-empty `reason="..."`; a bare skip fails
the gate. See CONTRIBUTING.md ("Documentation examples smoke gate") for the
contributor-facing rules.

Config file (default: scripts/docs-smoke.yaml):

    docs:
      - README.md
      - QUICKSTART.md
      - SPEC.md
      - docs/INDEX.md
    smoke_commands:
      - shatter --version
      - shatter --help

Usage:
    python3 scripts/docs-smoke.py [--config PATH] [--shatter-bin PATH] [-v]

The CLI binary is located via (in order): --shatter-bin, $SHATTER_BIN,
target/release/shatter, target/debug/shatter, then `shatter` on PATH. The gate
fails with a clear message if no binary can be found.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shlex
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parent

DEFAULT_CONFIG_PATH = SCRIPT_DIR / "docs-smoke.yaml"

SHELL_LANGS = frozenset(("bash", "sh", "shell", "console", "shell-session"))
JSON_LANGS = frozenset(("json",))
YAML_LANGS = frozenset(("yaml", "yml"))

# Binary invocation tokens that introduce a `shatter` command line.
SHATTER_BIN_TOKENS = frozenset(
    (
        "shatter",
        "./target/release/shatter",
        "./target/debug/shatter",
        "target/release/shatter",
        "target/debug/shatter",
    )
)

# Flags clap always provides, on every command.
UNIVERSAL_LONG = frozenset(("--help", "--version"))
UNIVERSAL_SHORT = frozenset(("-h", "-V"))

DIRECTIVE_RE = re.compile(r"<!--\s*docs-smoke:\s*(?P<body>.*?)\s*-->")
REASON_RE = re.compile(r'reason\s*=\s*"(?P<reason>[^"]*)"')
FENCE_RE = re.compile(r"^(?P<indent>\s*)(?P<ticks>`{3,})(?P<info>.*)$")


# ---------------------------------------------------------------------------
# Result accumulator
# ---------------------------------------------------------------------------


@dataclass
class Result:
    errors: list[str] = field(default_factory=list)
    warnings: list[str] = field(default_factory=list)

    def error(self, msg: str) -> None:
        self.errors.append(msg)

    def warn(self, msg: str) -> None:
        self.warnings.append(msg)

    def ok(self) -> bool:
        return not self.errors


# ---------------------------------------------------------------------------
# Fenced-block extraction
# ---------------------------------------------------------------------------


@dataclass
class Block:
    lang: str
    content: str
    start_line: int  # 1-based line of the opening fence
    skip: bool = False
    skip_reason: str | None = None
    directive_error: str | None = None


def extract_directive(prev_nonblank: str | None) -> tuple[bool, str | None, str | None]:
    """Parse a docs-smoke directive from the line above a fence.

    Returns (skip, reason, error). `error` is non-None when a directive is
    malformed (e.g. skip with no reason), which the caller turns into a gate
    failure so exemptions cannot be added without justification.
    """
    if prev_nonblank is None:
        return (False, None, None)
    m = DIRECTIVE_RE.search(prev_nonblank)
    if not m:
        return (False, None, None)
    body = m.group("body").strip()
    verb = body.split()[0] if body else ""
    if verb != "skip":
        return (False, None, f"unknown docs-smoke directive '{verb}' (only 'skip' is supported)")
    reason_m = REASON_RE.search(body)
    if not reason_m or not reason_m.group("reason").strip():
        return (True, None, "docs-smoke: skip directive requires a non-empty reason=\"...\"")
    return (True, reason_m.group("reason").strip(), None)


def parse_fenced_blocks(text: str) -> list[Block]:
    """Extract fenced code blocks, honoring a docs-smoke directive placed on
    the nearest non-blank line above the opening fence."""
    lines = text.splitlines()
    blocks: list[Block] = []
    i = 0
    n = len(lines)
    while i < n:
        m = FENCE_RE.match(lines[i])
        if not m:
            i += 1
            continue
        fence_ticks = m.group("ticks")
        indent = m.group("indent")
        lang = m.group("info").strip().split()[0].lower() if m.group("info").strip() else ""
        start_line = i + 1

        # Directive: nearest non-blank line above the opening fence.
        prev_nonblank: str | None = None
        j = i - 1
        while j >= 0:
            if lines[j].strip():
                prev_nonblank = lines[j]
                break
            j -= 1
        skip, reason, derr = extract_directive(prev_nonblank)

        # Collect body until a closing fence of >= the same length.
        body_lines: list[str] = []
        i += 1
        closed = False
        while i < n:
            cm = FENCE_RE.match(lines[i])
            if cm and len(cm.group("ticks")) >= len(fence_ticks) and not cm.group("info").strip():
                closed = True
                i += 1
                break
            body_lines.append(lines[i])
            i += 1
        # Strip a common indent equal to the fence indent for tidy content.
        content = "\n".join(body_lines)
        blocks.append(
            Block(
                lang=lang,
                content=content,
                start_line=start_line,
                skip=skip,
                skip_reason=reason,
                directive_error=derr,
            )
        )
        if not closed:
            break
    return blocks


# ---------------------------------------------------------------------------
# Shell invocation parsing + validation
# ---------------------------------------------------------------------------


def parse_shell_commands(content: str) -> list[list[str]]:
    """Return tokenized `shatter` invocations from a shell code block.

    Only lines whose first token (after an optional `$ ` prompt) is a known
    shatter binary token are parsed. Output lines, comments, prompts, and other
    commands (cargo/git/curl/...) are ignored, so mixed command+output blocks
    do not produce false positives.
    """
    commands: list[list[str]] = []
    for raw in content.splitlines():
        s = raw.strip()
        if not s:
            continue
        if s.startswith("$ "):
            s = s[2:].lstrip()
        elif s == "$":
            continue
        first = s.split(maxsplit=1)[0] if s else ""
        if first not in SHATTER_BIN_TOKENS:
            continue
        try:
            tokens = shlex.split(s, comments=True)
        except ValueError:
            # Unbalanced quotes etc. — skip rather than crash the gate.
            continue
        if tokens:
            commands.append(tokens)
    return commands


@dataclass
class CliSpec:
    command_paths: set[tuple[str, ...]] = field(default_factory=set)
    long_flags: dict[tuple[str, ...], set[str]] = field(default_factory=dict)
    short_flags: dict[tuple[str, ...], set[str]] = field(default_factory=dict)
    global_long: set[str] = field(default_factory=set)
    global_short: set[str] = field(default_factory=set)

    def top_level(self) -> set[str]:
        return {p[0] for p in self.command_paths if len(p) == 1}


def _parse_help_flags(help_text: str) -> tuple[set[str], set[str]]:
    """Extract (long, short) flags from the Options: section of a --help dump."""
    longs: set[str] = set()
    shorts: set[str] = set()
    idx = help_text.find("Options:")
    section = help_text[idx:] if idx != -1 else help_text
    for line in section.splitlines():
        if not line.strip().startswith("-"):
            continue
        # The flag column is everything before the 2+ space gap to the desc.
        flag_col = re.split(r"\s{2,}", line.strip(), maxsplit=1)[0]
        for lm in re.findall(r"--([a-zA-Z][a-zA-Z0-9-]*)", flag_col):
            longs.add("--" + lm)
        for sm in re.findall(r"(?:^|[\s,])(-[a-zA-Z])(?:[\s,]|$)", flag_col):
            shorts.add(sm)
    return (longs, shorts)


def _parse_help_subcommands(help_text: str) -> list[str]:
    """Extract subcommand names from the Commands: section of a --help dump."""
    idx = help_text.find("Commands:")
    if idx == -1:
        return []
    section = help_text[idx + len("Commands:"):]
    # Scan command rows until the next top-level section (Options:/Arguments:).
    names: list[str] = []
    for line in section.splitlines():
        if line.strip() in ("Options:", "Arguments:"):
            break
        m = re.match(r"\s+([a-z][a-z0-9-]*)(?:\s{2,}|$)", line)
        if m:
            name = m.group(1)
            if name != "help":
                names.append(name)
    return names


def _run_help(bin_path: str, path: tuple[str, ...]) -> str | None:
    try:
        proc = subprocess.run(
            [bin_path, *path, "--help"],
            capture_output=True,
            text=True,
            timeout=60,
        )
    except (OSError, subprocess.TimeoutExpired):
        return None
    return (proc.stdout or "") + "\n" + (proc.stderr or "")


def build_cli_spec(bin_path: str, verbose: bool = False) -> CliSpec:
    """Introspect the built CLI's help to map command paths to their flags."""
    spec = CliSpec()
    root_help = _run_help(bin_path, ())
    if root_help is None:
        raise RuntimeError(f"could not run '{bin_path} --help'")
    spec.global_long, spec.global_short = _parse_help_flags(root_help)

    # BFS over the subcommand tree.
    frontier: list[tuple[str, ...]] = [(name,) for name in _parse_help_subcommands(root_help)]
    seen: set[tuple[str, ...]] = set()
    while frontier:
        path = frontier.pop()
        if path in seen:
            continue
        seen.add(path)
        help_text = _run_help(bin_path, path)
        if help_text is None:
            continue
        spec.command_paths.add(path)
        longs, shorts = _parse_help_flags(help_text)
        spec.long_flags[path] = longs
        spec.short_flags[path] = shorts
        for child in _parse_help_subcommands(help_text):
            frontier.append(path + (child,))
    if verbose:
        print(f"  CLI spec: {len(spec.command_paths)} command paths, "
              f"{len(spec.global_long)} global long flags")
    return spec


def check_shatter_invocation(tokens: list[str], spec: CliSpec) -> list[str]:
    """Validate one tokenized `shatter ...` invocation against the CLI spec.

    Returns a list of human-readable error strings (empty when valid). Pure
    function — tests inject a synthetic CliSpec.
    """
    errors: list[str] = []
    if not tokens:
        return errors

    # Resolve the longest known command path from tokens[1:].
    path: tuple[str, ...] = ()
    i = 1
    while i < len(tokens) and not tokens[i].startswith("-"):
        cand = path + (tokens[i],)
        if cand in spec.command_paths:
            path = cand
            i += 1
        else:
            break

    # If the first non-flag token is not a known (sub)command, it is either a
    # removed command or a positional. Only flag it when there is a top-level
    # command set to compare against and the token clearly looks like a command
    # that used to exist (i.e. tokens[1] with no path resolved).
    if path == () and len(tokens) > 1 and not tokens[1].startswith("-"):
        if (tokens[1],) not in spec.command_paths and spec.command_paths:
            errors.append(
                f"unknown subcommand '{tokens[1]}' in `{' '.join(tokens)}` "
                f"— not a current shatter command (removed or renamed?)"
            )
            return errors

    allowed_long = set(UNIVERSAL_LONG) | spec.global_long | spec.long_flags.get(path, set())
    allowed_short = set(UNIVERSAL_SHORT) | spec.global_short | spec.short_flags.get(path, set())

    cmd_label = "shatter" + ("" if not path else " " + " ".join(path))
    for tok in tokens[i:]:
        if tok == "--":
            break
        if tok.startswith("--"):
            name = tok.split("=", 1)[0]
            if name not in allowed_long:
                errors.append(
                    f"unknown flag '{name}' for `{cmd_label}` in "
                    f"`{' '.join(tokens)}` (stale or removed flag?)"
                )
        elif tok.startswith("-") and len(tok) > 1 and not tok[1].isdigit():
            # Possibly a bundle of short flags (e.g. -vv). Accept if every
            # component is a known short flag.
            components = ["-" + c for c in tok[1:]]
            if not all(c in allowed_short for c in components) and tok not in allowed_short:
                errors.append(
                    f"unknown short flag '{tok}' for `{cmd_label}` in "
                    f"`{' '.join(tokens)}` (stale or removed flag?)"
                )
    return errors


# ---------------------------------------------------------------------------
# JSON / YAML validation
# ---------------------------------------------------------------------------


def validate_json_block(content: str) -> str | None:
    """Return an error string if content is not valid JSON, else None."""
    try:
        json.loads(content)
        return None
    except json.JSONDecodeError as exc:
        return f"invalid JSON: {exc}"


def validate_yaml_block(content: str) -> str | None:
    """Return an error string if content is not valid YAML, else None."""
    try:
        import yaml
    except ImportError:
        return "pyyaml is required to validate YAML examples — pip install pyyaml"
    try:
        yaml.safe_load(content)
        return None
    except yaml.YAMLError as exc:
        return f"invalid YAML: {exc}"


# ---------------------------------------------------------------------------
# Doc validation orchestration
# ---------------------------------------------------------------------------


def validate_doc(
    doc_path: Path,
    rel_label: str,
    spec: CliSpec,
    result: Result,
    verbose: bool,
) -> None:
    text = doc_path.read_text()
    blocks = parse_fenced_blocks(text)
    n_checked = 0
    n_skipped = 0
    for block in blocks:
        loc = f"{rel_label}:{block.start_line}"

        if block.directive_error is not None:
            result.error(f"{loc}: {block.directive_error}")
            continue

        if block.skip:
            n_skipped += 1
            if verbose:
                print(f"  {loc}: skip ({block.lang or 'plain'}) — {block.skip_reason}")
            continue

        if block.lang in SHELL_LANGS:
            for tokens in parse_shell_commands(block.content):
                for err in check_shatter_invocation(tokens, spec):
                    result.error(f"{loc}: {err}")
                n_checked += 1
        elif block.lang in JSON_LANGS:
            err = validate_json_block(block.content)
            if err:
                result.error(f"{loc}: {err}")
            n_checked += 1
        elif block.lang in YAML_LANGS:
            err = validate_yaml_block(block.content)
            if err:
                result.error(f"{loc}: {err}")
            n_checked += 1
        # Other langs (ts, markdown, plain/untagged) are not validated.
    if verbose:
        print(f"  {rel_label}: {len(blocks)} blocks, {n_checked} checked, {n_skipped} skipped")


def run_smoke_commands(
    commands: list[str],
    bin_path: str,
    result: Result,
    verbose: bool,
) -> None:
    """Execute allowlisted, self-contained commands to prove the CLI is live."""
    if not commands:
        return
    with tempfile.TemporaryDirectory(prefix="docs-smoke-") as tmp:
        env = dict(os.environ)
        env.setdefault("NO_COLOR", "1")
        for cmd in commands:
            tokens = shlex.split(cmd)
            if not tokens:
                continue
            # Replace a leading `shatter` token with the resolved binary.
            if tokens[0] in SHATTER_BIN_TOKENS:
                argv = [bin_path, *tokens[1:]]
            else:
                argv = tokens
            try:
                proc = subprocess.run(
                    argv,
                    cwd=tmp,
                    env=env,
                    capture_output=True,
                    text=True,
                    timeout=120,
                )
            except (OSError, subprocess.TimeoutExpired) as exc:
                result.error(f"smoke_command '{cmd}': failed to run ({exc})")
                continue
            if proc.returncode != 0:
                tail = (proc.stderr or proc.stdout or "").strip().splitlines()
                snippet = tail[-1] if tail else "(no output)"
                result.error(
                    f"smoke_command '{cmd}': exited {proc.returncode} — {snippet}"
                )
            elif verbose:
                print(f"  smoke_command '{cmd}': ok")


# ---------------------------------------------------------------------------
# Config + binary discovery
# ---------------------------------------------------------------------------


def load_config(path: Path) -> dict:
    try:
        import yaml
    except ImportError:
        print("ERROR: pyyaml is required — pip install pyyaml", file=sys.stderr)
        sys.exit(2)
    if not path.exists():
        print(f"ERROR: docs-smoke config not found at: {path}", file=sys.stderr)
        sys.exit(2)
    data = yaml.safe_load(path.read_text())
    if not isinstance(data, dict):
        print(f"ERROR: docs-smoke config at {path} must be a YAML mapping", file=sys.stderr)
        sys.exit(2)
    if not isinstance(data.get("docs"), list) or not data["docs"]:
        print(f"ERROR: docs-smoke config at {path} must list at least one doc under 'docs:'",
              file=sys.stderr)
        sys.exit(2)
    return data


def find_shatter_bin(explicit: str | None) -> str | None:
    candidates: list[str] = []
    if explicit:
        candidates.append(explicit)
    env_bin = os.environ.get("SHATTER_BIN")
    if env_bin:
        candidates.append(env_bin)
    candidates.append(str(REPO_ROOT / "target" / "release" / "shatter"))
    candidates.append(str(REPO_ROOT / "target" / "debug" / "shatter"))
    for c in candidates:
        p = Path(c)
        if p.exists() and os.access(p, os.X_OK):
            return str(p)
    # Fall back to PATH.
    from shutil import which
    on_path = which("shatter")
    return on_path


# ---------------------------------------------------------------------------
# Entry point
# ---------------------------------------------------------------------------


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--config", type=Path, default=DEFAULT_CONFIG_PATH,
                        help=f"Path to docs-smoke config YAML (default: {DEFAULT_CONFIG_PATH})")
    parser.add_argument("--shatter-bin", default=None,
                        help="Path to the built shatter binary (default: auto-detect)")
    parser.add_argument("--verbose", "-v", action="store_true",
                        help="Show per-block detail")
    parser.add_argument("--skip-run", action="store_true",
                        help="Skip execution of smoke_commands (static checks only)")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    verbose: bool = args.verbose

    config = load_config(args.config)
    docs: list[str] = config["docs"]
    smoke_commands: list[str] = config.get("smoke_commands", []) or []

    bin_path = find_shatter_bin(args.shatter_bin)
    if bin_path is None:
        print(
            "ERROR: could not find a built `shatter` binary. Build it with\n"
            "  cargo build -p shatter-cli\n"
            "or pass --shatter-bin PATH / set SHATTER_BIN.",
            file=sys.stderr,
        )
        return 2

    print(f"Docs smoke gate — CLI binary: {bin_path}")
    try:
        spec = build_cli_spec(bin_path, verbose=verbose)
    except RuntimeError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2

    result = Result()

    print(f"\nValidating {len(docs)} doc(s)...")
    for rel in docs:
        doc_path = (REPO_ROOT / rel).resolve()
        if not doc_path.exists():
            result.error(f"{rel}: configured doc not found")
            continue
        validate_doc(doc_path, rel, spec, result, verbose)

    if smoke_commands and not args.skip_run:
        print(f"\nRunning {len(smoke_commands)} smoke command(s)...")
        run_smoke_commands(smoke_commands, bin_path, result, verbose)
    elif args.skip_run:
        print("\n[skip] smoke_commands execution disabled (--skip-run)")

    print()
    if result.warnings:
        print("Warnings:")
        for w in result.warnings:
            print(f"  warn: {w}")
        print()

    if result.errors:
        print(f"ERRORS ({len(result.errors)} failure(s)):")
        for e in result.errors:
            print(f"  FAIL: {e}")
        print()
        print("Docs smoke gate FAILED.")
        return 1

    print("Docs smoke gate passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
