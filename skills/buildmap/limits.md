# What the map costs, and what it still cannot see

The ratchet over what cannot be resolved, the per-turn cost, the query mode, and the blind spots that stay
on purpose. Rules for reading the map are in [SKILL.md](SKILL.md).

## `--map-baseline` - a ratchet over what the map cannot resolve

A name built at run time is the one import this map cannot resolve, and each is a blind spot every dead-file
finding is qualified by (the `NO READER` text names the count). So they are gated:

```
structuregate --root . --ext .py --map --map-check --map-baseline map-baseline.json
structuregate --root . --ext .py --map --map-baseline map-baseline.json --update-map-baseline
```

* a site NOT recorded is new dynamic code - rejected (`DYNAMIC`);
* a recorded site may not GROW;
* a recorded site that is GONE must be REMOVED, so the list can only shrink.

Without the third rule a baseline is a permanent exemption list. **The limit is 0**: there is no acceptable
number, only the number already there. **Keyed by file and shape, never by line** (`app.py:
importlib.import_module() with a name built at run time`) - a baseline that churned on every unrelated edit
would be switched off inside a week. **One list, four syntaxes**: `. $lib`, `import(expr)`,
`importlib.import_module(name)`, `Type.GetType(name)` ratchet together.

**`unread`, the second list** - every file nothing in the tree imports (`UNREAD`): dead, or reached in a way
no parse tree sees (an HTTP route, a shell script, a registry keyed by a string). Three such ways are now
read, each a name the tree already carries rather than a guess:

* a dotted import this root cannot resolve is emitted as a path SUFFIX and joined against every root, so
  `pkg.sub` in one tree finds the file in another instead of stopping at the package `__init__`;
* a string literal exactly spelling a declared module is an OPTIONAL edge - only where the string is USED AS
  A KEY (a `getattr`/`__import__` argument, a module-level registry), never a dict LOOKUP's key: emitted from
  everywhere, an ordinary word like `"tasks"` drew dozens of edges out of dict keys and log messages;
* a def handed to an object at import time (`@app.route`, `@cli.command`) marks its file `registered`.

An upgraded gate therefore reports FEWER unread files, and the ratchet asks for the list to be re-recorded.
Most of what stays is named by no string anywhere: parsing harder will not resolve it; what is worth
refusing is the next one arriving unnoticed.

## Cheap enough for a Stop hook

`--map-if-stale` re-parses NOTHING when the map is newer than every mapped file, the file set matches and
the exe is older than the map. The walk still runs, so the answer is honest; the parsers do not, so it costs
milliseconds. With `--map-check` the kept map is still CHECKED - its recorded findings decide the exit, so a
red map stays red next turn. A `--map-baseline` newer than the map re-parses. When something did move, only
what moved is parsed again ([outputs.md](outputs.md)): an edit turn drops from seconds to a fraction of one.

```jsonc
// .claude/settings.json
{ "hooks": { "Stop": [ { "hooks": [ {
  "type": "command",
  "command": "buildtools\\structuregate.exe --root . --tracked --include-untracked --ext .py --map --map-if-stale --map-check"
} ] } ] } }
```

A cold run is one host launch per half (`python`, `powershell.exe`, `node`) - seconds over a few hundred
files. Errors go to **stderr**, where a Claude Code hook shows them: a non-blocking hook discards stdout, so
a verdict printed there arrives as "No stderr output". Nothing reads `buildmap.json` at run time and no build
step consumes it, so a stale copy can give a wrong ANSWER, never a wrong artifact - which is what makes
rebuilding it every turn safe. (A hook-wired tree runs `buildtools/StructureGate.Hook.ps1` instead, for the
exit-2 translation - see the `gate-connect` skill.)

## `--map-query` - the lenses, and no file in your tree

```
structuregate --map-query map.sqlite --tables
structuregate --map-query map.sqlite --text "cache AND key"
structuregate --map-query map.sqlite --cat core/config.py --lines 1-4
structuregate --map-query map.sqlite --sql "SELECT callee, count(*) FROM calls GROUP BY callee"
```

Every lens is listed in the `map-sqlite` skill ([../map-sqlite/SKILL.md](../map-sqlite/SKILL.md)). **Why a mode
of the exe and not a script on disk:** a query script dropped into a consumer's tree is a file that tree then
owns - the map lists it, the gate counts it against the folder limit, and the unread ratchet asks who imports
it. `--cat` and `--text` are the point: the file map POINTS at files and never carries them, so a question
that needs to see the code had nowhere else to go.

## The blind spots that remain, and why each stays

Each is REPORTED rather than silently absent - the difference between a map you can act on and one you
cannot:

* **A name built at run time**, in every language - counted into `computed_imports`, never guessed. A guessed
  edge cannot be told from a proven one.
* **A name declared in two files** - `AMBIGUOUS`, no edge to either. Which one a use binds to is scope and
  compilation order, which this tool does not ask (with `--map-sqlite`, a C# name is joined to the file the
  compiler bound it to).
* **A caller outside the mapped tree** - the `NO READER` note says so, because `--ext` and `--root` decide
  what "the tree" means.
* **A language with no parser here** - listed per file as `UNMAPPED`, with the reason.
* **What only running the code knows** - an artifact registry, a settings overlay. That is `--map-plugin`'s
  job ([edges.md](edges.md)), never the parser's.

**Multi-root trees resolve per root.** A half answers in paths relative to the root it was GIVEN, so with
several `--root` trees each import is resolved against its own (`import util` → `lib/util.py`, not a BROKEN
`util.py`). And a case-insensitive filesystem's "yes" to a name differing only in case is not trusted:
`from pkg import CONFIG` does not draw an edge to `pkg/config.py`. Both were found comparing `--map` edge for
edge against a hand-written map script, which it then reproduced in full (every import edge, the same
`AMBIGUOUS` names, every duplicate-expression group) and exceeded: a dotted import resolved by package path,
not bare name.
