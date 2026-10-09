---
name: map-findings-issues
description: File a project's source-map findings (what structuregate-map's Angular, python, C# or SQL map gets wrong or leaves out, tagged map-gap / tool-gap) as PUBLIC GitHub issues on the tool's repo, blogic-cz/structuregate-map by default - only a generic `public` title and body each finding carries, never the consumer's names, paths, ids, values or code. Edits an issue when its public text changes and closes it when the finding is resolved. Also files findings about a library a consumer depends on via --repo/--tags/--label. Use when the user says "push the findings to GitHub", "file the map gaps as issues", "report this map gap to structuregate", "sync findings with issues", "update the issue", or after recording a new map-gap finding that the map's owner should fix.
---

# Map findings -> public GitHub issues

A consumer keeps what a source map gets wrong in its own findings file, in its own terms. The map is
built in a PUBLIC repository, so an issue there is readable by anyone and indexed by search engines.
`push_findings.py` beside this file mirrors the findings as issues - **and sends nothing concrete**.

```bash
python <skill dir>/push_findings.py --findings <findings.json>           # dry run
python <skill dir>/push_findings.py --findings <findings.json> --write   # apply
```

## Nothing concrete: the `public` part

A finding's `text`, `targets`, `evidence`, id and the project name are NEVER sent. Only its `public`
part is, and a finding without one is skipped (`SKIP`):

```json
{"id": "F-0043", "status": "open", "tags": ["map-gap"], "text": "<the consumer's own words>",
 "public": {"title": "A directive that renders through a method called in a callback gets no restriction",
            "body": "Repro: ... a few lines of SYNTHETIC code ... Expected: ... Actual: ...",
            "resolution": "optional closing note",
            "attachments": ["public/repro.ts"]}}
```

Write the public part as if for a stranger:

- **A minimal synthetic repro** the owner can paste into a fixture: made-up names (`Demo.*`,
  `OrderService`, `Alpha`/`Beta`/`Gamma`, `KindIDs`), a few lines, the expected and the actual rows.
- **Never** the consumer's project, product, client, company or people names, its paths, file names,
  row or gate ids, enum members, business values, translation keys, SQL objects, log lines, screenshots
  or pasted code - not even "as the consumer writes it". Restate the SHAPE, not the instance.
- Numbers only when they are the point (a count that is wrong), never ones that identify the consumer.

**The deny list is the safety net.** `public-deny.txt` beside the findings file (or `--deny <file>`)
holds one term per line - the consumer's name, its projects, domain words, path fragments. A public part
containing any of them, case-insensitively, is `REFUSE`d. Keep the list in the consumer; it is never sent.

## Flags

| flag | default | meaning |
| --- | --- | --- |
| `--repo` | `blogic-cz/structuregate-map` | the repo that owns the map |
| `--tags` | `map-gap,tool-gap` | a finding carrying any of them is considered |
| `--account` | the active gh account | the gh account whose token runs `gh` |
| `--deny` | `public-deny.txt` beside the findings | terms a public part may not contain |
| `--label` | `map-finding` | the label every filed issue carries, and the prefix of its marker |
| `--project` | the findings file's git checkout name | hashed into the marker only, never shown |
| `--files-branch` | `findings-files` | the branch of `--repo` that holds attached files |

## What it does

- **Open finding with a public part, no issue** -> creates an issue titled with `public.title`, body
  `public.body`, labelled `--label` plus the finding's matching tags.
- **Open finding, open issue whose title or body differs** -> EDIT from the public part.
- **Resolved / wontfix finding, open issue** -> closes it with `public.resolution` (or a generic note).
- **Everything else** -> KEEP. An issue closed while the finding is still open is not reopened.

Each issue is found again by an OPAQUE marker, `<!-- <label>: <hash> -->` (a hash of project and finding
id), so a re-run never duplicates and the issue names neither. Keep `--label` and `--project` the same
on every run, or every finding is filed again.

## Attached files

Only files `public.attachments` lists, relative to the findings file. GitHub has no API for issue
attachments, so each is committed to the `--files-branch` branch of `--repo` under
`<label>/<hash>/<name>` - as public as the issue. Attach only synthetic files you wrote for the issue.

## Before running with --write

1. Run it dry and read every CREATE / EDIT / CLOSE / UPLOAD / REFUSE / SKIP line, then read each public
   title and body once more as a stranger would. When in doubt, generalise further.
2. A public body must state a REPRO the owner can run. Rewrite one that only describes a symptom.
3. Never file a finding that is not about the map, the tool or the library named by `--repo`: a
   consumer's own bug is fixed in the consumer.

`gh` must be logged in as an account that can open issues on the repo (`gh auth status`).
