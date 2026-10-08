"""Files a finding attaches, kept on a branch of the issue's repo so the issue can link them.

GitHub has no API for an issue attachment (the web UI's drag-and-drop upload is not exposed), so each
file is committed through the contents API to one branch of the same repo, `findings-files` by
default, under `<label>/<key>/<name>` - `<key>` an opaque hash of project and finding id, so the branch names
neither (the repo is public). Everyone who can see the repo can open it, and an
image renders inline in the issue. The branch is an orphan - it shares no history with the code.

A file is uploaded only when its git blob sha differs from the branch's copy, so a re-run uploads
nothing new.
"""
from __future__ import annotations

import base64
import hashlib
import json
import os
import subprocess
from pathlib import Path
from urllib.parse import quote

IMAGE_EXTS = {".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp"}
README = ("Files attached to issues filed by the `map-findings-issues` skill, one folder per finding:\n"
          "`<label>/<key>/`, the key an opaque hash. The consumer's findings file is the source of truth.\n")


def _run(args: list, token: str, stdin: str = "") -> subprocess.CompletedProcess:
    env = {**os.environ, "GH_TOKEN": token} if token else None
    return subprocess.run(["gh", *args], capture_output=True, text=True, encoding="utf-8", env=env, input=stdin)


def gh(args: list, token: str, check: bool = True) -> str:
    r = _run(args, token)
    if check and r.returncode != 0:
        raise SystemExit(f"gh {' '.join(args[:3])} failed: {r.stderr.strip()}")
    return r.stdout


def _api(path: str, token: str, method: str = "GET", payload: dict | None = None):
    """JSON of a REST call, or None on 404."""
    args = ["api", "-X", method, path]
    if payload is not None:
        args += ["--input", "-"]
    r = _run(args, token, json.dumps(payload) if payload is not None else "")
    if r.returncode != 0:
        if "HTTP 404" in r.stderr:
            return None
        raise SystemExit(f"gh api {method} {path} failed: {r.stderr.strip()}")
    return json.loads(r.stdout) if r.stdout.strip() else {}


def _blob_sha(data: bytes) -> str:
    return hashlib.sha1(b"blob %d\0" % len(data) + data).hexdigest()


def resolve(f: dict, base: Path) -> list[Path]:
    """The finding's `attachments`, relative to the findings file. Raises on a missing or doubled name."""
    paths = [p if p.is_absolute() else base / p for p in map(Path, f.get("attachments") or ())]
    missing = [str(p) for p in paths if not p.is_file()]
    if missing:
        raise SystemExit(f"{f['id']}: attachment not found: {', '.join(missing)}")
    names = [p.name for p in paths]
    if len(set(names)) != len(names):
        raise SystemExit(f"{f['id']}: two attachments share a file name: {', '.join(names)}")
    return paths


class FileStore:
    def __init__(self, repo: str, branch: str, label: str, project: str, token: str):
        self.repo, self.branch, self.label, self.project, self.token = repo, branch, label, project, token
        self._exists: bool | None = None

    def key(self, fid: str) -> str:
        return hashlib.sha256(f"{self.project}/{fid}".encode("utf-8")).hexdigest()[:16]

    def repo_path(self, fid: str, p: Path) -> str:
        return f"{self.label}/{self.key(fid)}/{p.name}"

    def url(self, fid: str, p: Path) -> str:
        return f"https://github.com/{self.repo}/blob/{quote(self.branch)}/{quote(self.repo_path(fid, p))}"

    def markdown(self, fid: str, paths: list[Path]) -> list[str]:
        """Links as one list, then each image as its own paragraph (a line under a list item joins it)."""
        out = [f"- [{p.name}]({self.url(fid, p)})" for p in paths if p.suffix.lower() not in IMAGE_EXTS]
        for p in paths:
            if p.suffix.lower() in IMAGE_EXTS:
                out += ["", f"![{p.name}]({self.url(fid, p)}?raw=true)"]
        return out[1:] if out and out[0] == "" else out

    def _branch_exists(self) -> bool:
        if self._exists is None:
            self._exists = _api(f"repos/{self.repo}/branches/{quote(self.branch)}", self.token) is not None
        return self._exists

    def _create_branch(self) -> None:
        tree = _api(f"repos/{self.repo}/git/trees", self.token, "POST",
                    {"tree": [{"path": "README.md", "mode": "100644", "type": "blob", "content": README}]})
        commit = _api(f"repos/{self.repo}/git/commits", self.token, "POST",
                      {"message": "Start the findings files branch", "tree": tree["sha"], "parents": []})
        _api(f"repos/{self.repo}/git/refs", self.token, "POST",
             {"ref": f"refs/heads/{self.branch}", "sha": commit["sha"]})
        self._exists = True

    def pending(self, fid: str, paths: list[Path]) -> list[tuple[Path, str | None]]:
        """(file, sha of the branch's copy or None) for each file the branch lacks or holds stale."""
        out = []
        for p in paths:
            remote = None
            if self._branch_exists():
                remote = _api(f"repos/{self.repo}/contents/{quote(self.repo_path(fid, p))}"
                              f"?ref={quote(self.branch)}", self.token)
            if remote is None or remote.get("sha") != _blob_sha(p.read_bytes()):
                out.append((p, remote.get("sha") if remote else None))
        return out

    def upload(self, fid: str, p: Path, old_sha: str | None) -> None:
        if not self._branch_exists():
            self._create_branch()
        payload = {"message": f"{self.key(fid)}: {p.name}", "branch": self.branch,
                   "content": base64.b64encode(p.read_bytes()).decode("ascii")}
        if old_sha:
            payload["sha"] = old_sha
        _api(f"repos/{self.repo}/contents/{quote(self.repo_path(fid, p))}", self.token, "PUT", payload)
