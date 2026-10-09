"""Mirror a project's findings into GitHub issues on the repo that owns them (default: structuregate-map).

THE ISSUES ARE PUBLIC, SO NOTHING CONCRETE GOES INTO THEM. A consumer records what a source map gets
wrong in a findings file - a JSON list of {"id", "status", "tags", "text", "targets", "evidence", ...},
written in the consumer's own terms. None of that is sent. Only a finding's `public` part is:
{"title": "...", "body": "...", "attachments": [...]}, written generically - a minimal synthetic repro
(`Demo.*`, `Alpha/Beta`, a few lines of made-up code), never the consumer's names, paths, ids, values,
code or data. A finding without a `public` part is SKIPPED, and a public text holding any term of the
deny list (`--deny`, default `public-deny.txt` beside the findings file: one term per line, matched
case-insensitively) is REFUSED.

Each OPEN finding tagged `map-gap` / `tool-gap` (or the tags you pass) becomes one issue, is edited when
its public part changes, and is closed when the finding is resolved. One issue per finding, found again
by an OPAQUE marker in its body (`<!-- <label>: <hash> -->`, a hash of project and id), so a re-run never
duplicates and the issue names neither. Attached files (only those the public part lists) go to a
branch of the same repo (issue_files.py), which is as public as the issue.

    python push_findings.py --findings path/to/findings.json                 # dry run: what it would do
    python push_findings.py --findings path/to/findings.json --write         # create / edit / close issues
    python push_findings.py --findings f.json --repo owner/name --tags map-gap
    python push_findings.py --findings f.json --repo <owner>/<library> \\
        --tags library-bug --label library-finding                          # findings about a library

Runs `gh` with the token of `--account` (default: the active gh account), so the active account is left
alone when another one is named.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

from issue_files import FileStore, gh as _gh, resolve

DEFAULT_REPO = "blogic-cz/structuregate-map"
DEFAULT_TAGS = ("map-gap", "tool-gap")
DEFAULT_LABEL = "map-finding"
DEFAULT_FILES_BRANCH = "findings-files"
LABEL_COLOR = "1d76db"
TITLE_LEN = 90
CLOSED_STATUSES = ("resolved", "wontfix", "closed")


def _marker(label: str, project: str, fid: str) -> str:
    # OPAQUE: the marker is in the issue's source, so it must not spell the project or the finding id.
    digest = hashlib.sha256(f"{project}/{fid}".encode("utf-8")).hexdigest()[:16]
    return f"<!-- {label}: {digest} -->"


def _token(account: str | None) -> str:
    args = ["gh", "auth", "token"] + (["--user", account] if account else [])
    r = subprocess.run(args, capture_output=True, text=True)
    if r.returncode != 0:
        raise SystemExit(f"no gh login{' for account ' + repr(account) if account else ''} - run: gh auth login")
    return r.stdout.strip()


def _public(f: dict) -> dict:
    p = f.get("public")
    return p if isinstance(p, dict) and str(p.get("title") or "").strip() and str(p.get("body") or "").strip() else {}


def _denied(text: str, deny: list[str]) -> list[str]:
    low = text.lower()
    return [t for t in deny if t.lower() in low]


def _project(findings: Path) -> str:
    r = subprocess.run(["git", "-C", str(findings.parent), "rev-parse", "--show-toplevel"],
                       capture_output=True, text=True)
    return Path(r.stdout.strip()).name if r.returncode == 0 and r.stdout.strip() else findings.parent.name


def _title(f: dict) -> str:
    first = " ".join(str(_public(f).get("title") or "").split())
    return first if len(first) <= TITLE_LEN else first[:TITLE_LEN - 1].rstrip() + "…"


def _body(label: str, project: str, f: dict, files: list[str] = ()) -> str:
    lines = [_marker(label, project, f["id"]), "", str(_public(f).get("body") or "").strip()]
    if files:
        lines += ["", "**Files:**", "", *files]
    lines += ["", "_Filed from a consumer's findings by the `map-findings-issues` skill._"]
    return "\n".join(lines)


def _same(a: str, b: str) -> bool:
    return a.replace("\r\n", "\n").strip() == b.replace("\r\n", "\n").strip()


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--findings", type=Path, required=True, help="the project's findings JSON (a list)")
    ap.add_argument("--repo", default=DEFAULT_REPO, help=f"owner/name of the tool's repo (default {DEFAULT_REPO})")
    ap.add_argument("--project", help="name the issues carry (default: the findings file's git checkout)")
    ap.add_argument("--tags", default=",".join(DEFAULT_TAGS), help="comma list; a finding with any of them is filed")
    ap.add_argument("--account", help="gh account to act as (default: the active gh account)")
    ap.add_argument("--deny", type=Path, help="terms a public text may not contain, one per line "
                                               "(default: public-deny.txt beside the findings file)")
    ap.add_argument("--label", default=DEFAULT_LABEL,
                    help=f"label every filed issue carries; also the marker's prefix (default {DEFAULT_LABEL})")
    ap.add_argument("--files-branch", default=DEFAULT_FILES_BRANCH,
                    help=f"branch of --repo that holds attached files (default {DEFAULT_FILES_BRANCH})")
    ap.add_argument("--write", action="store_true", help="create, edit and close issues (default: report only)")
    args = ap.parse_args(argv)

    findings = json.loads(args.findings.read_text(encoding="utf-8"))
    if not isinstance(findings, list):
        raise SystemExit("the findings file must be a JSON list of findings")
    tags = {t.strip() for t in args.tags.split(",") if t.strip()}
    deny_file = args.deny or args.findings.parent / "public-deny.txt"
    deny = [t.strip() for t in deny_file.read_text(encoding="utf-8").splitlines()
            if t.strip() and not t.strip().startswith("#")] if deny_file.exists() else []
    if not deny:
        print(f"WARNING: no deny list at {deny_file} - only the public texts' own wording keeps the consumer out")
    tagged = [f for f in findings if f.get("id") and set(f.get("tags") or ()) & tags]
    selected, refused = [], 0
    for f in tagged:
        pub = _public(f)
        is_open = str(f.get("status") or "open").lower() not in CLOSED_STATUSES
        if is_open and not pub:
            print(f"SKIP    {f['id']}: no public part - write `public.title` and `public.body` with nothing concrete")
            continue
        hits = _denied(" ".join([str(pub.get("title") or ""), str(pub.get("body") or ""),
                                 *map(str, pub.get("attachments") or [])]), deny) if pub else []
        if hits:
            print(f"REFUSE  {f['id']}: its public part names {', '.join(sorted(set(hits)))}")
            refused += 1
            continue
        selected.append(f)
    attachments = {f["id"]: resolve({"attachments": _public(f).get("attachments") or []}, args.findings.parent)
                   for f in selected}
    project = args.project or _project(args.findings)
    token = _token(args.account)
    store = FileStore(args.repo, args.files_branch, args.label, project, token)

    existing = json.loads(_gh(["issue", "list", "-R", args.repo, "--state", "all", "--limit", "1000",
                               "--search", f"{args.label} in:body", "--json", "number,state,title,body,url"], token))
    by_marker = {}
    for issue in existing:
        for f in selected:
            if _marker(args.label, project, f["id"]) in (issue.get("body") or ""):
                by_marker[f["id"]] = issue

    if args.write:
        _gh(["label", "create", args.label, "-R", args.repo, "--color", LABEL_COLOR,
             "--description", "Filed from a consumer's findings file"],
            token, check=False)
        for t in sorted(tags):
            _gh(["label", "create", t, "-R", args.repo, "--color", LABEL_COLOR], token, check=False)

    created = edited = closed = kept = uploaded = 0
    for f in selected:
        ftags = set(f.get("tags") or ())
        issue = by_marker.get(f["id"])
        is_open = str(f.get("status") or "open").lower() not in CLOSED_STATUSES
        files = attachments[f["id"]]
        title = _title(f) or f"(closed finding)"
        body = _body(args.label, project, f, store.markdown(f["id"], files))
        if is_open and (not issue or issue["state"] == "OPEN"):
            # Upload first, so the issue never links a file the branch does not hold yet.
            for p, old_sha in store.pending(f["id"], files):
                print(f"UPLOAD  {f['id']} {p.name} -> {args.files_branch}:{store.repo_path(f['id'], p)}")
                if args.write:
                    store.upload(f["id"], p, old_sha)
                uploaded += 1
        if is_open and not issue:
            print(f"CREATE  {title}")
            if args.write:
                url = _gh(["issue", "create", "-R", args.repo, "--title", title, "--body", body,
                           "--label", ",".join([args.label, *sorted(ftags & tags)])], token).strip()
                print(f"        {url}")
            created += 1
        elif is_open and issue["state"] == "OPEN" and not (
                _same(issue.get("title") or "", title) and _same(issue.get("body") or "", body)):
            print(f"EDIT    #{issue['number']} {f['id']}")
            if args.write:
                _gh(["issue", "edit", str(issue["number"]), "-R", args.repo, "--title", title, "--body", body], token)
            edited += 1
        elif not is_open and issue and issue["state"] == "OPEN":
            print(f"CLOSE   #{issue['number']} {f['id']} (finding is {f.get('status')})")
            if args.write:
                note = str(_public(f).get("resolution") or "").strip() or "Resolved in the consumer's findings."
                _gh(["issue", "close", str(issue["number"]), "-R", args.repo, "--comment", note], token)
            closed += 1
        else:
            where = f"#{issue['number']} {issue['state'].lower()}" if issue else "no issue"
            print(f"KEEP    {f['id']} ({f.get('status')}, {where})")
            kept += 1
    mode = "done" if args.write else "dry run - add --write to apply"
    print(f"\n{created} to create, {edited} to edit, {closed} to close, {kept} unchanged, {refused} refused, "
          f"{uploaded} files to upload on {args.repo} ({mode})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
