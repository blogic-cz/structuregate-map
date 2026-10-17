//! WHAT SHAPES THE ROWS NODE'S PAYLOAD BECOMES, hashed into the library - so a release that changes only what rust
//! DERIVES from those rows (the closure, the stylesheet tables) does not make every consumer read its whole
//! TypeScript tree again.
//!
//! The Angular half's rows are node's payload as rust stores it. node's own share of that is its scripts, its `rows`
//! number, its compilers and its config - all in its setup. Rust's share is the code that hands node what it is given
//! (its arguments, the state, the carried rows) and turns the payload into stored rows (the parse, the cells, the
//! insert, the ids, the claims, the graph a partial run plans with). That code is LISTED here, and its bytes are the
//! digest `NODE_ROWS` (`src/embedded/node_rows.rs`) folds into node's setup and the half's skip key: a release that
//! touches a listed file reads a tree again; one that does not keeps the stored rows and derives again only what rust
//! derives from them (`rows/derive/tsderive.rs`).
//!
//! EVERY FILE A LISTED FILE NAMES IS LISTED OR NAMED BELOW AS NOT SHAPING THE ROWS, with why: the unit test
//! `node_rows::every_module_the_listed_code_names_is_classified` walks the crate's modules and fails on one that is
//! neither - a new module the write path starts calling cannot go unhashed. A folder entry covers every file under it,
//! a file added there included; a test file (`tests.rs`, `*_tests.rs`) is never hashed, it ships nothing.

use std::path::{Path, PathBuf};

/// The rust code a stored node row passes through, under `src/` (a folder ends in `/`).
const LISTED: &[(&str, &str)] = &[
    ("mapper/deep/ts.rs", "node's arguments, the state and line counts it is handed, which run (full, partial, every hop) is asked"),
    ("mapper/deep/flight.rs", "the state handed to node in flight: the id lane its rows are numbered in"),
    ("rows/mod.rs", "the state (`ts_state`): shas, counters, `alone`, the graph and the setup node plans with"),
    ("rows/calls.rs", "the payload mapped and parsed, the carry and the state handed over"),
    ("rows/rawcells.rs", "the payload's rows, streamed cell by cell"),
    ("rows/tsapply.rs", "the write path: the replace, the spec and provenance, the indexes, what a partial run keeps"),
    ("rows/store.rs", "the insert, `file_text`, the counters and `_meta`"),
    ("rows/schema.rs", "each table's columns, widened, and its indexes"),
    ("rows/half.rs", "what a run deletes, and whether its ids restart (`alone`)"),
    ("rows/pyjson.rs", "the spec and provenance text, and the newline rule `file_text` reads by"),
    ("rows/dups.rs", "the column and owner SQL the reads proof asks every row with (`reads::unread`)"),
    ("rows/cells/", "every structure a row holds, as nodes"),
    ("rows/partial/", "the carry, the claims, the graph (`ts_deps`, `ts_scope`), the reads proof, what a partial run replaces"),
];

/// Files a listed file names that do NOT shape a stored node row - or that a listed folder holds and that only
/// derive. Every one says why; a folder covers what it holds.
const NOT_SHAPING: &[(&str, &str)] = &[
    ("rows/derive/", "the stylesheet tables and the closure, derived from the stored rows; whether a partial run keeps them"),
    ("mapper/deep/derived.rs", "when the stored rows are derived again, and the passes over the finished rows"),
    ("rows/ts/", "the closure and its gate readers - derived"),
    ("cssmap/", "the stylesheet tables and `class_hides` - derived"),
    ("rows/fts.rs", "the row search index - a pass over the finished rows"),
    ("atlas.rs", "the atlas - a pass over the finished rows"),
    ("rows/http/", "the http pass and the token it is keyed by - a pass over the finished rows"),
    ("mapper/deep/mod.rs", "the half's options and keys: their values are in the key, not in a row"),
    ("mapper/deep/payload.rs", "where a skip key is kept"),
    ("mapper/protocol.rs", "what a run reports"),
    ("embedded/", "the scripts node runs, whose digest is in node's setup already"),
    ("hosts/", "starts a process; what node writes is its own"),
    ("trace/", "spans and timings"),
    ("lib.rs", "the crate root"),
];

fn main() {
    let src = Path::new(&std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR")).join("src");
    println!("cargo:rerun-if-changed=build.rs");
    let mut files: Vec<String> = Vec::new();
    for (entry, _) in LISTED {
        println!("cargo:rerun-if-changed=src/{entry}");
        collect(&src, entry, &mut files);
    }
    files.retain(|f| !NOT_SHAPING.iter().any(|(n, _)| covers(n, f)));
    files.sort();
    files.dedup();
    let mut hash = Fnv::new();
    for file in &files {
        let bytes = std::fs::read(src.join(file)).unwrap_or_else(|e| panic!("src/{file} could not be read: {e}"));
        hash.update(file.as_bytes());
        hash.update(&[0]);
        // A CHECKOUT'S LINE ENDINGS ARE NOT THE CODE: `\r\n` and `\n` hash alike.
        let mut prev_cr = false;
        for &b in &bytes {
            if prev_cr && b != b'\n' {
                hash.update(b"\r");
            }
            prev_cr = b == b'\r';
            if !prev_cr {
                hash.update(&[b]);
            }
        }
        hash.update(&[0]);
    }
    let mut out = String::new();
    out.push_str(&format!("/// The digest of every listed file (`build.rs`).\npub const DIGEST: &str = \"{}\";\n", hash.hex()));
    out.push_str("/// Every file hashed into `DIGEST`, under `src/`.\n#[cfg_attr(not(test), allow(dead_code))]\npub const FILES: &[&str] = &[\n");
    for file in &files {
        out.push_str(&format!("    {file:?},\n"));
    }
    out.push_str("];\n/// The list as written: `(file or folder/, why)`.\n#[cfg_attr(not(test), allow(dead_code))]\npub const LISTED: &[(&str, &str)] = &[\n");
    for (entry, why) in LISTED {
        out.push_str(&format!("    ({entry:?}, {why:?}),\n"));
    }
    out.push_str("];\n/// What a listed file may name without being listed.\n#[cfg_attr(not(test), allow(dead_code))]\npub const NOT_SHAPING: &[(&str, &str)] = &[\n");
    for (entry, why) in NOT_SHAPING {
        out.push_str(&format!("    ({entry:?}, {why:?}),\n"));
    }
    out.push_str("];\n");
    let target = PathBuf::from(std::env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("node_rows.rs");
    std::fs::write(&target, out).unwrap_or_else(|e| panic!("{} could not be written: {e}", target.display()));
}

/// Whether `entry` (a file, or a folder ending in `/`) covers `file`.
fn covers(entry: &str, file: &str) -> bool {
    if entry.ends_with('/') { file.starts_with(entry) } else { file == entry }
}

/// A test file ships nothing: never hashed.
fn is_test(name: &str) -> bool {
    name == "tests.rs" || name.ends_with("_tests.rs")
}

/// Every `.rs` file an entry names, relative to `src/` with `/` between - a folder's at any depth.
fn collect(src: &Path, entry: &str, into: &mut Vec<String>) {
    let at = src.join(entry);
    if !entry.ends_with('/') {
        assert!(at.is_file(), "build.rs lists src/{entry}, which is not a file");
        into.push(entry.to_string());
        return;
    }
    assert!(at.is_dir(), "build.rs lists src/{entry}, which is not a folder");
    let mut stack = vec![(at, entry.to_string())];
    while let Some((dir, rel)) = stack.pop() {
        let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("src/{rel} could not be listed: {e}")).flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for item in entries {
            let name = item.file_name().to_string_lossy().into_owned();
            let path = item.path();
            if path.is_dir() {
                stack.push((path, format!("{rel}{name}/")));
            } else if name.ends_with(".rs") && !is_test(&name) {
                into.push(format!("{rel}{name}"));
            }
        }
    }
}

/// FNV-1a, two lanes of 64 bits: a fixed function of the bytes on every toolchain (std's hasher promises no such
/// thing), with nothing to add to the build's dependencies. It tells releases apart; nobody forges it.
struct Fnv(u64, u64);

impl Fnv {
    fn new() -> Fnv {
        Fnv(0xcbf2_9ce4_8422_2325, 0x6c62_272e_07bb_0142)
    }

    fn update(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
            self.1 = (self.1 ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn hex(&self) -> String {
        format!("{:016x}{:016x}", self.0, self.1)
    }
}
