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
  folder, `#[path]` is relative to the declaring file). Every other path segment is a NAME.
- Only items another file can name are declared (`pub`, `pub(crate)`, … and every `macro_rules!`). A
  `#[cfg(test)]` module declares nothing. A method after `.` is a member, never a use.
- `proc_macro2::extra::invalidate_current_thread_spans()` runs after every file. Outside a proc macro the
  lexer keeps every file's text in a thread-local map for the life of the thread otherwise.
- A file the lexer refuses falls back to the `//` scanner for its count and is `UNPARSED` in the map.

`#[path]` IS HOW A FOLDER IS SPLIT without moving a module: `rows/ts/gate/` and `rows/ts/key/` keep every
`super::` meaning `rows::ts`. `store_tests.rs` is `store::tests` the same way.

THE DEEP MAP'S RUST ROWS (`--map-sqlite`) are `files`, `consts` and `handlers`, from `rsmap/consts.rs` and
`rsmap/handlers.rs` (one parse, `rows_of`) stored by
`mapper/deep/rust.rs` through `rows::apply` with `lang = 'rust'` - incremental by sha like the python half.
A constant is a module-level, inline-`mod`, `impl` or `trait` `const`/`static`; one in a function body or a
`#[cfg(test)]` module is not. A handler is every way an error is dealt with - each `?`, `Err` arm, `if let Err`,
`let Ok(..) else`, `unwrap`/`expect`, fallback, `.ok()`, `map_err`, `let _ =` and `catch_unwind` - named by `shape`,
with `test` set inside a `#[cfg(test)]` module or `#[test]` fn. No type checker: an Option's `.unwrap()` is a row too. `string_literals` and
`number_literals` (`rsmap/literals.rs`) carry the `use`/`callee`/`target` every half shares for `--magic`; syn keeps
no parent, so a parent hands its child the use, and a macro's body is parsed as expressions where it can be. **Bump `ROWS_VERSION` in `rust.rs` with every change to what it writes**, or
an unchanged file keeps the old rows.

THE REGEX BAN searches rust RECURSIVELY (`findstr /S` over each crate's `src`), unlike every other glob
in `BanRegex` — a new module folder is covered without anyone adding it.
