---
paths:
  - "rust/**"
  - "src/Map/Fbt/**"
  - "tests/Rust.Tests.ps1"
---

# The rust half, and rust in this repo

`.rs` is measured here like every other source (`--ext` in `src/StructureGate.csproj`). Files that were
already over a limit when it was turned on are in `structure-baseline.json` and may only shrink.

THE HALF IS `rust/fbtcore/src/rsmap/`, parsed by `syn` INSIDE the exe — the second in-process half after
Roslyn. It is called in process: its line counter by `count/` (the gate) and `map_file` by
`mapper/halves.rs` (the map). EVERY entry point belongs in its own module's `mod.rs` (`cli/`, `gate/run.rs`, `mapper/`, `rows/seeds/`): `lib.rs` holds only the handle and the store doors the C# halves call back through.

- A source line is a line a TOKEN sits on. `proc-macro2` returns `///` and `//!` as `#[doc]` tokens;
  they are told apart from a typed attribute by the SOURCE under the `#` (a comment starts with `/`).
- `mod name;` is an exact edge, resolved like rustc (`modpath.rs`: crate roots and `mod.rs` own their
  folder, `#[path]` is relative to the declaring file). A path's ROOT decides what its later segments name (`rsmap/paths.rs`): `crate::`/`super::` and an in-tree `use` name them, `std::`, `Self::`, an own item and `TokenTree::Group` after an extern import name nothing, and a name through another crate joins only into that crate. AN IN-TREE PATH IS WALKED through the module tree (`graph/rspaths.rs`: `mod x;` and inline modules, `crate`/`super`/`self` anchors) and lands in the file of the module it reaches, following a module's imports and in-tree globs at an item (a re-export, a child's `use super::*;`); only an unresolved bare name is joined by name.
- Only items another file can name are declared (`pub`, `pub(crate)`, … and every `macro_rules!`). A
  `#[cfg(test)]` item or module declares nothing, and a bare name a `let`, parameter or pattern binds is a local, never a use. A method after `.` is a member, never a use.
- `proc_macro2::extra::invalidate_current_thread_spans()` runs after every file. Outside a proc macro the
  lexer keeps every file's text in a thread-local map for the life of the thread otherwise.
- A file the lexer refuses falls back to the `//` scanner for its count and is `UNPARSED` in the map.

`#[path]` IS HOW A FOLDER IS SPLIT without moving a module: `rows/ts/gate/` and `rows/ts/key/` keep every
`super::` meaning `rows::ts`. `store_tests.rs` is `store::tests` the same way.

THE DEEP MAP'S RUST ROWS (`--map-sqlite`) are `files`, `consts` and `handlers`, from `rsmap/consts.rs` and
`rsmap/handlers.rs` (one parse, `rows_of`) stored by
`mapper/deep/rust.rs` through `rows::apply` with `lang = 'rust'` - incremental by sha like the python half.
A constant is a module-level, inline-`mod`, `impl` or `trait` `const`/`static`, and a `thread_local!` static; one in a function body, a
`#[cfg(test)]` module, a `#[cfg(test)]` item or `impl`, and `const _` are not, nor is one in a file that is test code as a whole (below). A handler is every way an error is dealt with - each `?`, `Err` arm, `if let Err`,
`let Ok(..) else`, `unwrap`/`expect`, fallback, `.ok()`, `map_err`, `let _ =` and `catch_unwind` - named by `shape`,
inside a macro's arguments too, when they parse as expressions (or are the values of `json!`, `vec![x; n]` or `matches!`), with `test` set inside a `#[cfg(test)]` module or `#[test]` fn (a method too), and in every file a `#[cfg(test)] mod x;` declares (and the files it declares) or a `tests/` integration crate holds - decided by the declaration, so its sha carries the flag (`rsmap/testmods.rs`). No type checker: an Option's `.unwrap()` is a row too. `string_literals` and
`number_literals` (`rsmap/literals.rs`) carry the `use`/`callee`/`target` every half shares for `--magic`; syn keeps
no parent, so a parent hands its child the use, and a macro's body is parsed as expressions where it can be; `json!`, `vec![x; n]` and `matches!` bodies are read by their grammar (`literals::body_pieces`), any other unparsed body for its literal tokens; the format string of `write!`/`writeln!`/`assert!*` is at its std position (1, or 2 for `assert_eq!`); a constant's type is walked, so an array length is a number. **Bump `ROWS_VERSION` in `rust.rs` with every change to what it writes**, or
an unchanged file keeps the old rows.

THE REGEX BAN searches rust RECURSIVELY (`findstr /S` over each crate's `src`), unlike every other glob
in `BanRegex` — a new module folder is covered without anyone adding it.

`rows/http/` (`endpoints.rs`, `calls.rs`, `matching.rs`) is the pass `pass: http links` in `mapper/deep/mod.rs` that
joins the halves: C# `decorators` (`arg_values`, the CsRows fold of each attribute argument) and `classes.bases` give
`http_endpoints`; TypeScript `calls` to `@angular/common/http`, folded through `members`, `locals`, `consts` and the
callable's parameters, and C# `calls` bound to `System.Net.Http` (`cscalls.rs`, #143), give `http_calls` (a callable whose primitive's url reads a parameter is a wrapper and its
callers are the rows); `http_links` is their match. The three tables are DROPped and CREATEd by every run that does not
REPLAY them: the pass is keyed on what it reads (`rows/http/inputs.rs`: the build, every C# file's sha, and a token the TypeScript
store moves only when a row of `TS_INPUTS` may have - a partial run compares what it removed with what it wrote).
- A name joins only into the file's own crate and the in-tree crates its Cargo.toml depends on by `path` (directly or `workspace = true`; `rsmap/cargo.rs`); a `build.rs` is its own crate and sees only its `[build-dependencies]`; a file outside any package joins as before. `--map-if-stale` does not watch Cargo.toml.
