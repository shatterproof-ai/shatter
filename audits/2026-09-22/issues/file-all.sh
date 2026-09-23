#!/usr/bin/env bash
# file-all.sh -- the ONE filer for the reconciled audit 2026-09-22 issue set (D6).
#
# Reads every draft under issues/<repo>/<bucket>/NN-*.md (front matter + body),
# and files them into each repo's tracker:
#   1. the repo epic            (bd create --type epic / gh issue create)
#   2. new issues                (bd create --parent <epic> --body-file <body>)
#   3. blocked-by edges          (bd dep add <issue> --blocked-by <dep>)
#   4. note / reopen-note comments on existing issues, plus companion comments
#      embedded in new-issue drafts (bd comments add <id> -f <file> /
#      gh issue comment -R <repo> <n> --body-file <file>)
#   5. a slug -> id map comment on the epic once every draft of the repo is filed
#
# Modes (environment):
#   DRY_RUN=1   default. Prints every command; writes nothing; runs only
#               read-only probes (bd list, bd create --dry-run, gh auth/label list).
#   APPLY=1     actually file (asks for confirmation on /dev/tty unless YES=1).
#               APPLY=1 together with an explicit DRY_RUN=1 stays a dry run.
#   ONLY=shatter,bento   restrict to these repo sections
#               (shatter bento shatter-agents storystore bugshot dotfiles).
#   ALLOW_UNREVIEWED=1   also file buckets whose cross-check produced no verdict
#               (held by default, see UNREVIEWED_BUCKETS below).
#   INCLUDE_BLOCKERS=1   also file slugs with a BLOCKER cross-check finding
#               (held by default, see BLOCKER_SLUGS below).
#   HOLD="slug another-slug"   extra slugs to hold back this run.
#   BD_VALIDATE=0        in dry-run, skip the per-draft `bd create --dry-run` probe.
#   LEDGER=path          ledger file (default: issues/filed-ledger.tsv).
#   KEEP_TMP=1           keep the generated body/comment files and print their dir.
#
# Idempotent / resumable: every successful action appends one row to the
# ledger (key<TAB>id<TAB>repo<TAB>utc). Keys: epic:<repo>, <slug>,
# dep:<slug>><blocker-slug>, comment:<slug>, companion:<slug>:<target>,
# slugmap:<repo>. Rows already in the ledger are skipped, so a re-run after a
# partial failure (or with a hold lifted) continues where it stopped. A create
# whose ledger row was lost is recovered by exact-title match against issues
# created on/after 2026-09-23 in that tracker.
#
# Tracker notes:
#   - bd bodies go in via --body-file (never `bd create --file`, which splits on H2).
#   - blocked_by naming a note-to-existing slug resolves to that note's
#     existing_id (e.g. task-list-json-poisons-checksums -> str-qwua7.3).
#   - blocked_by on note/reopen-note drafts is an ordering constraint only
#     (the comment needs the new id); no dep edge is added on existing issues.
#   - storystore's bd is write-blocked until the v32->v53 migration; the
#     section is skipped with a message while `bd list` reports the refusal.
#   - dotfiles uses GitHub Issues (gh -R ketang/dotfiles); only labels that
#     already exist in the repo are applied.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export FILER_ISSUES_DIR="$HERE"
export FILER_LEDGER="${LEDGER:-$HERE/filed-ledger.tsv}"
export FILER_MODE=dry
if [[ "${APPLY:-0}" == "1" && "${DRY_RUN:-}" != "1" ]]; then FILER_MODE=apply; fi
if [[ "$FILER_MODE" == "apply" && "${YES:-0}" != "1" ]]; then
  echo "APPLY mode: this files issues/comments into real trackers (ONLY=${ONLY:-all})." >&2
  echo "Have you reviewed issues/INDEX.md 'Review before filing' and run issue-readiness-check?" >&2
  read -r -p "Type 'file' to continue: " ok </dev/tty
  [[ "$ok" == "file" ]] || { echo "aborted" >&2; exit 1; }
fi
command -v python3 >/dev/null || { echo "python3 required" >&2; exit 1; }
python3 -c 'import yaml' 2>/dev/null || { echo "python3 PyYAML required" >&2; exit 1; }

exec python3 - "$@" <<'PY'
import atexit, datetime, glob, json, os, re, shlex, shutil, subprocess, sys, tempfile
import yaml

ISSUES = os.environ["FILER_ISSUES_DIR"]
LEDGER = os.environ["FILER_LEDGER"]
APPLY = os.environ["FILER_MODE"] == "apply"
ONLY = [s.strip() for s in os.environ.get("ONLY", "").split(",") if s.strip()]
ALLOW_UNREVIEWED = os.environ.get("ALLOW_UNREVIEWED") == "1"
INCLUDE_BLOCKERS = os.environ.get("INCLUDE_BLOCKERS") == "1"
EXTRA_HOLD = set(os.environ.get("HOLD", "").split())
BD_VALIDATE = os.environ.get("BD_VALIDATE", "1") == "1" and not APPLY
TAG = "audit-2026-09-22"
SINCE = "2026-09-23"

REPOS = [  # order matters: epics/issues of each repo are filed in this order
    ("shatter", "bd", "/home/ketan/project/shatter", "Epic: Audit 2026-09-22 findings"),
    ("bento", "bd", "/home/ketan/project/bento", "Epic: Audit 2026-09-22 findings (bento)"),
    ("shatter-agents", "bd", "/home/ketan/project/shatter-agents", "Epic: Audit 2026-09-22 findings (shatter-agents plugin)"),
    ("storystore", "bd", "/home/ketan/project/storystore", "Epic: Audit 2026-09-22 findings (storystore)"),
    ("bugshot", "bd", "/home/ketan/project/bugshot", "Epic: Audit 2026-09-22 findings (bugshot)"),
    ("dotfiles", "gh", "ketang/dotfiles", "Epic: Audit 2026-09-22 findings (global agent guidance and hooks)"),
]
# Cross-check results (issues/crosscheck/*.md, 2026-09-23). Buckets with no
# usable verdict are held unless ALLOW_UNREVIEWED=1.
UNREVIEWED_BUCKETS = {
    "shatter-reports-and-specs": "cross-check pending (no review file)",
    "shatter-frontend-ts": "cross-check incomplete (no review file)",
    "shatter-frontend-rust": "cross-check produced no usable review (stop-hook takeover)",
    "shatter-protocol-parity": "cross-check skipped/incomplete (no review file)",
    "storystore-adoption-blockers": "cross-check pending (no review file)",
}
# Slugs with a BLOCKER finding: held unless INCLUDE_BLOCKERS=1.
BLOCKER_SLUGS = {
    "z3-mixed-int-real-sort-split": "duplicates open str-t854z; convert to a note",
    "float-constant-rational-conversion": "duplicates open str-aureo; convert to a note",
    "landing-deletes-remote-branches": "duplicates open bento-73de; convert to a note",
    "git-guard-bypasses-and-false-positives": "duplicates bento-l01v/i76i; rescope first",
}
TYPE_MAP = {"bug": "bug", "feature": "feature", "task": "task", "epic": "epic",
            "chore": "chore", "decision": "decision", "enhancement": "feature",
            "refactor": "task"}
GH_TYPE_LABEL = {"bug": "bug", "feature": "enhancement", "enhancement": "enhancement",
                 "task": None, "chore": None, "decision": None, "refactor": None, "epic": None}

errors, warnings, manual = [], [], []
TMP = tempfile.mkdtemp(prefix="file-all-")
if os.environ.get("KEEP_TMP") != "1": atexit.register(shutil.rmtree, TMP, True)
else: print(f"KEEP_TMP=1: bodies/comments kept in {TMP}")

def log(msg): print(msg, flush=True)
def err(msg): errors.append(msg); log("ERROR: " + msg)
def warn(msg): warnings.append(msg); log("WARN: " + msg)

# ---------------------------------------------------------------- ledger
ledger = {}
if os.path.exists(LEDGER):
    for line in open(LEDGER):
        parts = line.rstrip("\n").split("\t")
        if len(parts) >= 2 and parts[0] and not parts[0].startswith("#"):
            ledger[parts[0]] = parts[1]
def record(key, ident, repo):
    ledger[key] = ident
    if APPLY:
        new = not os.path.exists(LEDGER)
        with open(LEDGER, "a") as f:
            if new: f.write("#key\tid\trepo\tutc\n")
            f.write(f"{key}\t{ident}\t{repo}\t{datetime.datetime.utcnow().isoformat()}Z\n")
            f.flush(); os.fsync(f.fileno())

# ---------------------------------------------------------------- drafts
FM = re.compile(r"\A---\n(.*?)\n---\n", re.S)
drafts = []
for path in sorted(glob.glob(os.path.join(ISSUES, "*", "*", "[0-9]*.md"))):
    rel = os.path.relpath(path, ISSUES)
    repo, bucket = rel.split(os.sep)[:2]
    text = open(path).read()
    m = FM.match(text)
    if not m: err(f"{rel}: no front matter"); continue
    try: fm = yaml.safe_load(m.group(1))
    except yaml.YAMLError as e: err(f"{rel}: bad front matter: {e}"); continue
    for k in ("slug", "kind", "title", "priority", "type", "labels", "blocked_by", "existing_id"):
        if k not in fm: err(f"{rel}: front matter missing {k}")
    fm["_rel"], fm["_repo"], fm["_bucket"], fm["_body"] = rel, repo, bucket, text[m.end():]
    fm["blocked_by"] = fm.get("blocked_by") or []
    fm["labels"] = fm.get("labels") or []
    drafts.append(fm)
by_slug = {}
for d in drafts:
    if d["slug"] in by_slug: err(f"duplicate slug {d['slug']}: {d['_rel']} and {by_slug[d['slug']]['_rel']}")
    by_slug[d["slug"]] = d
for d in drafts:
    if d["kind"] not in ("new", "note-to-existing", "reopen-note"): err(f"{d['_rel']}: unknown kind {d['kind']}")
    if d["kind"] == "new" and d["type"] not in TYPE_MAP: err(f"{d['_rel']}: unmapped type {d['type']}")
    if d["kind"] != "new" and not str(d["existing_id"]).strip(): err(f"{d['_rel']}: note without existing_id")
    if not re.fullmatch(r"P[0-4]", str(d["priority"])): err(f"{d['_rel']}: bad priority {d['priority']}")
    for b in d["blocked_by"]:
        if b not in by_slug: err(f"{d['_rel']}: blocked_by unknown slug {b}")

def held_reason(d):
    if d["_bucket"] in UNREVIEWED_BUCKETS and not ALLOW_UNREVIEWED:
        return "HOLD-UNREVIEWED: " + UNREVIEWED_BUCKETS[d["_bucket"]]
    if d["slug"] in BLOCKER_SLUGS and not INCLUDE_BLOCKERS:
        return "HOLD-BLOCKER: " + BLOCKER_SLUGS[d["slug"]]
    if d["slug"] in EXTRA_HOLD: return "HOLD: listed in $HOLD"
    return None

# ---------------------------------------------------------------- text helpers
def strip_h1(body):
    lines = body.lstrip("\n").split("\n")
    if lines and lines[0].startswith("# "): lines = lines[1:]
    return "\n".join(lines).strip("\n") + "\n"

COMPANION_H2 = re.compile(r"^## (?:Comment for|Filer note \(companion one-liner on)\s+`?([a-z]+-[a-z0-9.]+)`?.*$")
def split_companions(body):
    """Remove companion-comment H2 sections from a new-issue body; return (body, [(target, text)])."""
    out, comps, cur = [], [], None
    for line in body.split("\n"):
        m = COMPANION_H2.match(line)
        if m:
            cur = [m.group(1), []]; comps.append(cur); continue
        if cur is not None and line.startswith("## "): cur = None
        (cur[1] if cur is not None else out).append(line)
    res = []
    for target, lines in comps:
        quoted = [l for l in lines if l.startswith(">")]
        res.append((target, unquote("\n".join(quoted)).strip() + "\n"))
    return "\n".join(out), res

def unquote(text):
    lines = text.split("\n")
    nonblank = [l for l in lines if l.strip()]
    if nonblank and all(l.startswith(">") for l in nonblank):
        lines = [re.sub(r"^> ?", "", l) for l in lines]
    return "\n".join(lines)

COMMENT_START = re.compile(r"^(?:#+\s*)?\**Comment text\b.*$")
COMMENT_END = re.compile(r"^(?:## |\(Filer|\**Filer)")
def extract_comment(d):
    lines = d["_body"].split("\n")
    for i, l in enumerate(lines):
        if COMMENT_START.match(l.strip()):
            j = i + 1
            while j < len(lines) and not COMMENT_END.match(lines[j]): j += 1
            txt = unquote("\n".join(lines[i + 1:j]).strip("\n")).strip()
            outside = "\n".join(lines[:i] + lines[j:])
            return txt + "\n", outside
    manual.append(f"{d['_rel']}: no 'Comment text' section; whole body posted as the comment")
    return strip_h1(d["_body"]), ""

def ident_of(slug, fake):
    """Tracker id for a slug: filed id, note's existing id, or None."""
    d = by_slug.get(slug)
    if d is None: return None
    if d["kind"] != "new": return str(d["existing_id"]).strip()
    if slug in ledger: return ledger[slug]
    return fake.get(slug)

PH = re.compile(r"<(?:id of )?([A-Za-z0-9][A-Za-z0-9._-]*)>")
def substitute(text, d, epic_id, fake):
    missing = []
    def rep(m):
        name = m.group(1)
        if name == "epic" and epic_id: return epic_id.lstrip("#") if m.start() > 0 and text[m.start() - 1] == "#" else epic_id
        if name == "NEW-ID":
            ids = [ident_of(b, fake) for b in d["blocked_by"]]
            if len(ids) == 1 and ids[0]: return ids[0]
            missing.append("NEW-ID"); return m.group(0)
        if name in by_slug:
            i = ident_of(name, fake)
            if i: return i
            missing.append(name)
        return m.group(0)
    return PH.sub(rep, text), missing

FILER_HINT = re.compile(r"filer|also add|widen|acceptance|priority|label|related link|re-?parent|close it|close this", re.I)

# ---------------------------------------------------------------- command runner
def show(cmd, cwd=None):
    return ("(cd %s && " % shlex.quote(cwd) if cwd else "") + shlex.join(cmd) + (")" if cwd else "")
def run(cmd, cwd=None, check=True, quiet=False):
    p = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True, stdin=subprocess.DEVNULL)
    if check and p.returncode != 0:
        raise RuntimeError(f"{show(cmd, cwd)} -> exit {p.returncode}: {(p.stderr or p.stdout).strip()[:800]}")
    return p
def act(cmd, cwd=None):
    """Mutating command: printed in dry-run, executed in apply. Returns stdout."""
    log(("RUN  " if APPLY else "DRY  ") + show(cmd, cwd))
    if not APPLY: return None
    return run(cmd, cwd).stdout

def write_tmp(name, text):
    p = os.path.join(TMP, re.sub(r"[^A-Za-z0-9._-]", "_", name))
    with open(p, "w") as f: f.write(text)
    return p

# ---------------------------------------------------------------- per repo
DECISIONS = """Maintainer decisions applied (2026-09-23):
- D1: keep Windows and aarch64-linux in the release matrix and fix them; release work closes only with a green release-run URL.
- D2: retire snapshot `shatter diff`; spec-diff is the regression tool.
- D3: measure concolic first (benchmark + early-termination fix), then re-decide positioning.
- D4: retire the beads JSONL import, sync via a Dolt remote; no hook-timeout env var or bypass guidance.
- D5: `[user]` already removed from .git/config; add .mailmap, a git-state check and a fixture config snapshot.
- D6: one filer script (this epic was created by audits/2026-09-22/issues/file-all.sh)."""

def epic_body(repo, n_new, n_notes):
    return (f"Parent epic for findings of the 2026-09-22 Shatter-led audit that belong to the {repo} tracker.\n\n"
            f"Drafts for this tracker: {n_new} new issues (children of this epic) and {n_notes} notes posted as comments on existing issues.\n\n"
            "Source: shatter branch `audit-2026-09-22`, `audits/2026-09-22/` (report, findings.json, "
            "issues/MANIFEST.md, issues/INDEX.md, per-bucket BUNDLE.md and crosscheck/ reviews). "
            "Issue bodies cite slugs of sibling drafts in backticks; the slug -> id map is posted as a "
            "comment on this epic once every draft for this repo is filed.\n\n" + DECISIONS + "\n")

def process(repo, kind, where, epic_title):
    items = [d for d in drafts if d["_repo"] == repo]
    new = [d for d in items if d["kind"] == "new"]
    notes = [d for d in items if d["kind"] != "new"]
    log(f"\n=== {repo} ({kind} {where}): {len(new)} new, {len(notes)} notes/reopen-notes ===")
    cwd = where if kind == "bd" else None
    fake = {}
    existing = {}   # id -> issue dict (bd) / number -> dict (gh)
    gh_labels = None

    # ---- preflight (read-only)
    try:
        if kind == "bd":
            p = run(["bd", "list", "--all", "--limit", "0", "--json"], cwd=cwd, check=False)
            blob = p.stdout + p.stderr
            if "refusing to auto-apply" in blob or "Writes are blocked" in blob:
                log(f"SKIP {repo}: tracker is still on schema v32 (bd writes blocked). Run "
                    "`BD_ALLOW_REMOTE_MIGRATE=1 bd migrate && bd dolt push` on the ONE designated clone "
                    "(maintainer approval required), then re-run with ONLY=" + repo + ".")
                return "skipped"
            if p.returncode != 0: raise RuntimeError(f"bd list failed in {cwd}: {blob.strip()[:400]}")
            for it in json.loads(p.stdout or "[]"): existing[it["id"]] = it
        else:
            run(["gh", "auth", "status"], check=True)
            p = run(["gh", "issue", "list", "-R", where, "--state", "all", "--limit", "1000",
                     "--json", "number,title,createdAt,state"])
            for it in json.loads(p.stdout): existing["#%d" % it["number"]] = dict(it, created_at=it["createdAt"], status=it["state"].lower())
            gh_labels = {l["name"] for l in json.loads(run(["gh", "label", "list", "-R", where, "--limit", "200", "--json", "name"]).stdout)}
    except Exception as e:
        err(f"{repo}: preflight failed: {e}"); return "failed"
    log(f"preflight ok: {len(existing)} existing issues readable")
    recent_by_title = {}
    for i, it in existing.items():
        if str(it.get("created_at", ""))[:10] >= SINCE: recent_by_title.setdefault(it["title"], []).append(i)

    # existing-id checks for notes
    for d in notes:
        eid = str(d["existing_id"]).strip()
        it = existing.get(eid)
        if it is None: err(f"{d['_rel']}: existing_id {eid} not found in {repo} tracker"); continue
        st = str(it.get("status", "")).lower()
        if d["kind"] == "reopen-note" and st not in ("closed",):
            warn(f"{d['_rel']}: reopen-note target {eid} is '{st}', expected closed")
        if d["kind"] == "note-to-existing" and st == "closed":
            warn(f"{d['_rel']}: note target {eid} is closed")

    def create(key, title, typ, prio, labels, body, parent):
        if key in ledger:
            log(f"skip {key}: already filed as {ledger[key]}"); return ledger[key]
        hits = recent_by_title.get(title, [])
        if len(hits) == 1:
            log(f"ADOPT {key}: exact-title issue {hits[0]} created since {SINCE} (ledger row was missing)")
            record(key, hits[0], repo); return hits[0]
        if len(hits) > 1: raise RuntimeError(f"{key}: {len(hits)} issues already titled {title!r}: {hits}; resolve by hand")
        bf = write_tmp(key + ".md", body)
        if kind == "bd":
            cmd = ["bd", "create", "--title", title, "--type", typ, "--priority", prio,
                   "--body-file", bf, "--external-ref", f"{TAG}:{key}", "--no-inherit-labels", "--silent"]
            if labels: cmd += ["--labels", ",".join(labels)]
            if parent: cmd += ["--parent", parent]
            if BD_VALIDATE:
                probe = [c for c in cmd if c != "--silent"]
                if parent:
                    k = probe.index("--parent"); del probe[k:k + 2]
                pr = run(probe + ["--dry-run"], cwd=cwd, check=False)
                if pr.returncode != 0: raise RuntimeError(f"bd create --dry-run rejected {key}: {(pr.stderr or pr.stdout).strip()[:400]}")
            out = act(cmd, cwd)
            if out is None: fake[key] = f"DRY:{key}"; return fake[key]
            ident = [l for l in out.strip().split("\n") if l.strip()][-1].strip()
            if not re.fullmatch(r"[a-z]+-[A-Za-z0-9.]+", ident): raise RuntimeError(f"unexpected bd create output for {key}: {out!r}")
        else:
            cmd = ["gh", "issue", "create", "-R", where, "--title", title, "--body-file", bf]
            for l in labels:
                if gh_labels is not None and l in gh_labels: cmd += ["--label", l]
            out = act(cmd)
            if out is None: fake[key] = f"#DRY-{key}"; return fake[key]
            m = re.search(r"/issues/(\d+)", out)
            if not m: raise RuntimeError(f"unexpected gh output for {key}: {out!r}")
            ident = "#" + m.group(1)
        log(f"  -> {ident}")
        record(key, ident, repo)
        return ident

    def comment(key, target, text):
        if key in ledger: log(f"skip {key}: already posted"); return
        f = write_tmp(key + ".comment.md", text)
        if kind == "bd": act(["bd", "comments", "add", target, "-f", f], cwd)
        else: act(["gh", "issue", "comment", "-R", where, target.lstrip("#"), "--body-file", f])
        record(key, target, repo)

    held = {d["slug"]: held_reason(d) for d in items if held_reason(d)}
    for s, r in held.items(): log(f"{r} -> {s}")

    try:
        # ---- 1. epic
        epic = create(f"epic:{repo}", epic_title, "epic", "P1", ["audit", TAG],
                      epic_body(repo, len(new), len(notes)), None)
        # ---- 2. new issues
        companions = []
        for d in new:
            if d["slug"] in held: continue
            body, comps = split_companions(strip_h1(d["_body"]))
            companions += [(d, t, c) for t, c in comps]
            body, _missing = substitute(body, d, epic, fake)   # forward slug refs stay as backticked slugs
            if kind == "gh":
                labels = list(d["labels"]) + ([GH_TYPE_LABEL[d["type"]]] if GH_TYPE_LABEL.get(d["type"]) else [])
                if epic not in body:
                    body = f"Part of {epic}. Priority: {d['priority']}. Type: {d['type']}.\n\n" + body
            else:
                labels = list(d["labels"])
            if re.search(r"\bfiler\b", body, re.I):
                manual.append(f"{d['_rel']}: body mentions the filer; check for instructions not automated")
            create(d["slug"], d["title"], TYPE_MAP[d["type"]], d["priority"], labels, body, epic if kind == "bd" else None)
        # ---- 3. dependencies
        for d in new:
            if d["slug"] in held: continue
            for b in d["blocked_by"]:
                key = f"dep:{d['slug']}>{b}"
                if key in ledger: log(f"skip {key}: already added"); continue
                me, dep = ident_of(d["slug"], fake), ident_of(b, fake)
                if by_slug[b]["_repo"] != repo:
                    manual.append(f"{d['_rel']}: cross-repo blocker {b} ({dep}); mention in body, bd cannot link it"); continue
                if not dep:
                    log(f"DEFER {key}: blocker {b} not filed ({held.get(b) or 'not yet filed'})"); continue
                if kind == "bd": act(["bd", "dep", "add", me, "--blocked-by", dep], cwd)
                else:
                    f = write_tmp(key + ".md", f"Blocked by {dep}.\n")
                    act(["gh", "issue", "comment", "-R", where, me.lstrip("#"), "--body-file", f])
                record(key, dep, repo)
        # ---- 4. comments on existing issues
        for d in notes:
            if d["slug"] in held: continue
            pending = [b for b in d["blocked_by"] if not ident_of(b, fake)]
            if pending:
                log(f"DEFER comment:{d['slug']}: ordering blocker(s) {pending} not filed "
                    f"({', '.join(held.get(x) or 'not yet filed' for x in pending)})"); continue
            text, outside = extract_comment(d)
            text, missing = substitute(text, d, epic, fake)
            if missing:
                log(f"DEFER comment:{d['slug']}: placeholders {sorted(set(missing))} have no id yet "
                    f"({', '.join(held.get(x) or 'not yet filed' for x in sorted(set(missing)))})"); continue
            if FILER_HINT.search(outside):
                manual.append(f"{d['_rel']}: filer/tracker instructions outside the comment text (links, AC/priority changes, extra comments) need a manual pass")
            comment(f"comment:{d['slug']}", str(d["existing_id"]).strip(), text)
        for d, target, text in companions:
            text, missing = substitute(text, d, epic, fake)
            if missing: log(f"DEFER companion:{d['slug']}:{target}: placeholders {missing}"); continue
            if target not in existing:
                err(f"{d['_rel']}: companion comment target {target} not found"); continue
            comment(f"companion:{d['slug']}:{target}", target, text)
        # ---- 5. slug map on the epic
        if held:
            log(f"slug map deferred: {len(held)} draft(s) held")
        elif f"slugmap:{repo}" not in ledger:
            rows = ["| Slug | Kind | Id | Title |", "|---|---|---|---|"]
            for d in items:
                rows.append(f"| {d['slug']} | {d['kind']} | {ident_of(d['slug'], fake)} | {d['title'].replace('|', '/')} |")
            comment(f"slugmap:{repo}", epic, "Audit 2026-09-22 slug -> id map (drafts cite sibling slugs in backticks):\n\n" + "\n".join(rows) + "\n")
    except Exception as e:
        err(f"{repo}: {e}  (re-run to resume; the ledger keeps completed steps)")
        return "failed"
    return "ok"

# ---------------------------------------------------------------- main
known = [r[0] for r in REPOS]
for o in ONLY:
    if o not in known: err(f"ONLY: unknown repo {o} (known: {' '.join(known)})")
log(f"mode={'APPLY' if APPLY else 'DRY_RUN'} ledger={LEDGER} ({len(ledger)} rows) drafts={len(drafts)} "
    f"ONLY={','.join(ONLY) or 'all'} ALLOW_UNREVIEWED={int(ALLOW_UNREVIEWED)} INCLUDE_BLOCKERS={int(INCLUDE_BLOCKERS)}")
# Evidence paths cite audits/2026-09-22/, which must be on shatter main first (MANIFEST filing notes).
p = run(["/usr/bin/git", "-C", "/home/ketan/project/shatter", "cat-file", "-e", "origin/main:audits/2026-09-22/findings.json"], check=False)
if p.returncode != 0:
    warn("audits/2026-09-22/ is not on shatter origin/main yet; land the audit reports (publish-audit-reports) before APPLY, or bodies cite unreachable paths")
status = {}
if not errors:
    for repo, kind, where, title in REPOS:
        if ONLY and repo not in ONLY: continue
        status[repo] = process(repo, kind, where, title)
log("\n=== summary ===")
for r, s in status.items(): log(f"{r}: {s}")
if manual:
    log("\nMANUAL follow-ups (not automated; do by hand after filing):")
    for m in manual: log("  - " + m)
if warnings: log(f"\n{len(warnings)} warning(s)")
if errors:
    log(f"\n{len(errors)} error(s):"); [log("  - " + e) for e in errors]
    sys.exit(1)
log("dry run clean" if not APPLY else "apply finished")
PY
