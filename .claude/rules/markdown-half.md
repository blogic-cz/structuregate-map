---
paths:
  - "rust/fbtcore/src/mdmap/**"
  - "src/Map/Fbt/MdMap.cs"
  - "tests/Markdown.Tests.ps1"
---

# The markdown half

THE HALF IS `rust/fbtcore/src/mdmap/`, parsed by `pulldown-cmark` INSIDE the exe - the third in-process
half after Roslyn and `syn`. Markdown has no host of its own, so its parser lives in the exe. One entry
point, `fbt_md_map`, called by `MdMap.Read` from `Map.cs`; frontmatter is read by `yaml-rust2`.

- WHICH DOCS: `Docs.InScope` - the `--doc-scope` set the gate measures, never `--ext`. `.md` in `--ext`
  would hold a doc to the SOURCE limit. The gate, the walk and the map share that one predicate.
- A DOC DECLARES NOTHING AND USES NOTHING. Its edges are `MapFile.Mentions` -> `mentions` /
  `mentioned_by` in the JSON, never `Uses`/`UsesPath`: a doc naming a dead file must not make it look read.
  A doc is `entry`, so it is never `NO READER` itself.
- WHAT IS A CLAIM (`names.rs::claimed`): a code span, a link target, or a word of a SHELL fence (`bash`,
  `powershell`, ...; the folder after `cd` too). An UNLABELLED fence is not one - across the connected
  trees it is as often a folder tree or a message. A `json`/`csharp`/`text` fence is example content. A
  placeholder, glob, variable, call, flag, URL or `/S` switch is not a path.
- WHERE IT LEADS (`names.rs::locate`, then `Tree::under`): under a `cd` earlier in the same fence, beside
  the doc, in EACH folder above it up to the root (a nested doc names paths from `src/`), then as a path
  SUFFIX under the doc's own folder (a doc names paths from the project inside it). The suffix step is
  why C# hands the half every file the walk saw. Lexical `..` only - `canonicalize` would answer through a
  junction with the target's name.
- WHAT IS MISSING: a link target; a shell word with a folder; a code span with a folder that ENDS IN A
  FILE NAME (`api/v1/login` is a route). Never a path through `bin/`/`obj/` - a build output. A bare name
  (`Map.cs`) is an edge when it is found and nothing when it is not.
- `DOC-MISSING` is a NOTE and only for a CONTEXT doc (`Docs.IsContextDoc`). A `docs/` page may describe
  another tree on purpose.

NOT YET: deep rows (`--map-sqlite`), and a doc shared into several trees (a junctioned skill) is checked
in none of them - the gate does not walk a junction. That is the `context-doc-audit` agent's job.
