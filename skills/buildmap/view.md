# The map as a page: `--map-view`

```bash
structuregate --map-view buildmap.sqlite                  # -> buildmap.html beside it
structuregate --map-view buildmap.sqlite --map-view-out map.html --map-view-graph buildmap.json
```

One self-contained HTML file: the data and every script inside it, nothing fetched, so it opens offline in
any tree the gate ships into. The libraries (sigma.js for WebGL, graphology, graphology-library for
ForceAtlas2 and Louvain, all MIT) are vendored in `src/MapView/vendor/` and compiled into the exe, about 430 KB.
`--map-view-out` and `--map-view-graph` are accepted only beside `--map-view <db>`.

## What it draws

Files and functions, with only the edges a half RESOLVED (`rust/fbtcore/src/view/model.rs`):

| edge | from |
|---|---|
| call, function → function | C# `calls.symbol` → `functions.symbol` (Roslyn); python and plain TypeScript `calls.target_path` + `target_name`. The caller is the innermost function whose lines hold the call |
| import, file → file | `imports.from_path` (python, plain TypeScript), and every import of the file map (`buildmap.json` beside the database, or `--map-view-graph`): rust `mod`, PowerShell dot-sources, TypeScript |
| reference, file → file | a C# `refs.symbol` naming a class (a field, a parameter, a `typeof`) |
| render, file → file | an Angular `render_graph` row |

A call into a file the map does not hold is not an edge; the page counts it as external.

## The four views

| view | answers |
|---|---|
| **Structure** | how the system is put together: folders as nodes, sized by lines, coloured by language, every edge rolled up to what is on screen. Double-click a folder to open it, right-click to close it. Colour by language, by cycle, by Louvain community |
| **Neighbourhood** | what breaks if I change this: a file or function and 1-4 hops of what it calls and what calls it. It stops at 2 500 nodes and says so |
| **Matrix** | layering: rows depend on columns, ordered so a dependency sits below what uses it. A red cell above the diagonal closes a cycle |
| **Treemap** | where the code is: area is lines, nested while there is room. Colour by language, by what nothing in the map reaches, by compile errors, by fan-in |

Every node shows its edges in both directions, and the `--map-query` command that prints its rows.

**Why not one big force graph.** WebGL renders 100 000 nodes fine, but nobody can read them. The page starts
at about 120 folders and files and opens more only when asked: a big tree is read top-down in Structure, a
question about one file or function in Neighbourhood.
