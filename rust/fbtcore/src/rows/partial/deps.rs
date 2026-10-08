//! WHICH FILES HAVE TO BE RE-EXTRACTED WHEN ONE CHANGES — the dependency graph over the map's
//! own rows.
//!
//! The reader decides what to parse and cannot read this database, so the graph is derived on
//! this side, stored in `_meta` by the run that writes the rows, and handed over in the state.
//! It is a view over rows the half already produced: nothing is parsed and nothing is guessed.
//!
//! THE GRAPH IS DERIVED MECHANICALLY FROM EVERY ID REFERENCE, never from a list of relations. A
//! row that POINTS at another row must be re-extracted when that row is, or its reference
//! dangles — so every id-valued cell in every table is an edge from the pointing row's file to
//! the pointed-at row's file. Naming the relations by hand was tried first and was WRONG on the
//! second sample: it missed `templates.class` (an `.html` file's row names the component class
//! in a `.ts` file, and neither imports the other) and `type_members.owner`. Measured against
//! the real map, the hand-written graph left about a third of the sampled files still referenced from
//! outside their own affected set; the mechanical one leaves none.
//!
//! IT IS NOT A COMPLETE ANSWER ON ITS OWN, and the caller must know why. The edges describe the
//! map AS IT IS, so they are valid only while the changed files keep resolving the same way. A
//! file that changes a component SELECTOR, or an NgModule's declarations, changes which
//! components a template resolves — an edge that does not exist yet and therefore cannot be in
//! this graph. The caller checks that separately, against `scope_files`, and falls back to a
//! full rebuild.

use indexmap::{IndexMap, IndexSet};
use serde_json::{Map, Value};

/// One row, as the payload holds it.
type Row = Map<String, Value>;

/// A row's own id and its housekeeping are not references to anywhere.
const SKIP_COLUMNS: &[&str] = &["id", "file", "half"];

/// The tables that say "this file declares something a template can name".
const SCOPE_TABLES: &[&str] = &["selector_index", "pipes", "directives", "ng_modules"];

/// `b:4711` — a short alphabetic prefix, a colon, digits. The shape every map
/// handle has.
///
/// No pattern: split and count. The prefix is bounded because these are the map's own handles,
/// and a source string that happens to contain a colon (`http://...`) fails the digit test.
fn looks_like_id(value: &str) -> bool {
    let Some(cut) = value.find(':') else { return false };
    if cut == 0 || cut > 3 || cut == value.len() - 1 {
        return false;
    }
    value[..cut].bytes().all(|b| b.is_ascii_alphabetic())
        && value[cut + 1..].bytes().all(|b| b.is_ascii_digit())
}

fn text(row: &Row, name: &str) -> Option<String> {
    match row.get(name) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// A list cell's strings - decoded, or still the text a payload sent (see `rawcells`).
pub(super) fn list(value: Option<&Value>) -> Vec<String> {
    let decoded;
    let value = match value {
        Some(v) => match super::rawcells::text_of(v) {
            Some(raw) => {
                decoded = serde_json::from_str::<Value>(raw).unwrap_or(Value::Null);
                &decoded
            }
            None => v,
        },
        None => return Vec::new(),
    };
    value.as_array().into_iter().flatten().filter_map(|s| s.as_str().map(String::from)).collect()
}

/// Every table's rows, as the payload holds them.
pub type Tables<'a> = &'a Map<String, Value>;

fn rows_of<'a>(tables: Tables<'a>, name: &str) -> impl Iterator<Item = &'a Row> {
    tables
        .get(name)
        .and_then(|v| v.as_array())
        .map(|a| a.as_slice())
        .unwrap_or(&[])
        .iter()
        .filter_map(|r| r.as_object())
}

/// `{path -> [the paths that must be re-extracted with it]}`, from the ROWS and not from the
/// database.
///
/// IT IS HANDED THE ROWS IT DERIVES FROM. Reading them back out of SQLite instead cost close
/// to A MINUTE on a large map — the database is hundreds of MB and had just been written, so nothing
/// was cached — against a rebuild the graph exists to make shorter.
///
/// IT SPEAKS PATHS, NOT ROW IDS, because the only consumer is the reader and it has no id for a
/// file until it has extracted one. A path is also what survives a rebuild: the ids are handles
/// this run made up.
pub fn graph(tables: Tables<'_>) -> IndexMap<String, Vec<String>> {
    let mut path_of: IndexMap<String, String> = IndexMap::new();
    for row in rows_of(tables, "files") {
        if let (Some(id), Some(path)) = (text(row, "id"), text(row, "path")) {
            path_of.insert(id, path);
        }
    }

    // Two passes, because the owner of an id is only known once every table has been seen: a row
    // in the first table may point at a row in the last.
    let mut owner: IndexMap<String, String> = IndexMap::new();
    for rows in tables.values() {
        for row in rows.as_array().map(|a| a.as_slice()).unwrap_or(&[]).iter().filter_map(|r| r.as_object()) {
            if let (Some(id), Some(file)) = (text(row, "id"), text(row, "file")) {
                owner.insert(id, file);
            }
        }
    }

    let mut out: IndexMap<String, IndexSet<String>> = IndexMap::new();
    for rows in tables.values() {
        for row in rows.as_array().map(|a| a.as_slice()).unwrap_or(&[]).iter().filter_map(|r| r.as_object()) {
            let Some(home) = text(row, "file") else { continue };
            for (column, value) in row {
                if SKIP_COLUMNS.contains(&column.as_str()) {
                    continue;
                }
                let Value::String(id) = value else { continue };
                if !looks_like_id(id) {
                    continue;
                }
                if let Some(target) = owner.get(id)
                    && *target != home
                {
                    out.entry(target.clone()).or_default().insert(home.clone());
                }
            }
        }
    }

    // A TEMPLATE IS EXTRACTED BY WALKING ITS COMPONENT, which is an edge in the OTHER direction
    // from every one above. The walk records that a row POINTING at another must be re-extracted
    // with it, and by that rule an `.html` depends on the `.ts` it names — true, and not the whole
    // truth: the template pass runs from the COMPONENT, so a changed `.html` cannot be re-extracted
    // without re-running the component that owns it. Left out, a changed template was parsed as an
    // ORPHAN instead: it read `layer=other` where the full run said `main`, lost its `reachable`,
    // and registered a second component of the same name.
    for row in rows_of(tables, "templates") {
        let home = text(row, "file");
        let component = text(row, "class").and_then(|c| owner.get(&c).cloned());
        if let (Some(home), Some(component)) = (home, component)
            && home != component
        {
            out.entry(home).or_default().insert(component);
        }
    }
    // AN IMPORT NAMES A FILE DIRECTLY rather than a row in one, so it is not an id reference and
    // the walk above cannot see it. It is the edge everything else is built on top of.
    for row in rows_of(tables, "imports") {
        if let (Some(importer), Some(imported)) = (text(row, "file"), text(row, "resolved_file"))
            && importer != imported
        {
            out.entry(imported).or_default().insert(importer);
        }
    }

    // A NAME IMPORTED THROUGH BARRELS depends on the file that declares it AND on every barrel on the way:
    // re-pointing any of them moves the declaration `import_names.declared` states, and an importer the
    // graph did not know about kept the stale one. The barrels are paths (`via`); the declaring file an id.
    let id_of_path: IndexMap<&String, &String> = path_of.iter().map(|(id, path)| (path, id)).collect();
    for row in rows_of(tables, "import_names") {
        let Some(importer) = text(row, "file") else { continue };
        let mut on_the_way: Vec<String> = text(row, "declared_file").into_iter().collect();
        for path in list(row.get("via")) {
            if let Some(id) = id_of_path.get(&path) {
                on_the_way.push((*id).clone());
            }
        }
        for target in on_the_way {
            if target != importer {
                out.entry(target).or_default().insert(importer.clone());
            }
        }
    }

    // TO PATHS, dropping any file the `files` table does not name — a row pointing at a file with
    // no path is a row the reader could not act on anyway, and inventing a key for it would make
    // the graph claim an edge to nowhere.
    let mut named: IndexMap<String, Vec<String>> = IndexMap::new();
    for (target, dependents) in out {
        let Some(target_path) = path_of.get(&target) else { continue };
        let mut paths: Vec<String> = dependents
            .iter()
            .filter_map(|d| path_of.get(d))
            .filter(|p| *p != target_path)
            .cloned()
            .collect();
        if paths.is_empty() {
            continue;
        }
        paths.sort();
        paths.dedup();
        named.insert(target_path.clone(), paths);
    }
    named
}

/// Every file that DECLARES something a template resolves by name — the guard's whole input.
///
/// THE GRAPH CANNOT COVER A SELECTOR THAT DOES NOT EXIST YET. Its edges describe the map as it
/// is, so they say which templates render a component TODAY. Change that component's `selector`,
/// or an NgModule's `declarations`, and a template that matched nothing may now match it — an
/// edge no recorded graph can hold, because the fact it would describe has not happened.
///
/// IT IS DELIBERATELY COARSE. The alternative re-reads the changed file's decorators and compares
/// the selector to the recorded one, which means computing the same fingerprint on both sides and
/// keeping them identical — and every cross-language parity surface in this half has cost a
/// defect. This costs nothing and cannot be subtly wrong. Measured: a small share of a large
/// workspace; the rest stays incremental.
pub fn scope_files(tables: Tables<'_>) -> Vec<String> {
    let mut path_of: IndexMap<String, String> = IndexMap::new();
    for row in rows_of(tables, "files") {
        if let (Some(id), Some(path)) = (text(row, "id"), text(row, "path")) {
            path_of.insert(id, path);
        }
    }
    let mut out: IndexSet<String> = IndexSet::new();
    for table in SCOPE_TABLES {
        for row in rows_of(tables, table) {
            if let Some(named) = text(row, "file").and_then(|f| path_of.get(&f).cloned()) {
                out.insert(named);
            }
        }
    }
    let mut paths: Vec<String> = out.into_iter().collect();
    paths.sort();
    paths
}

/// The stored graph with the re-extracted files' edges recomputed, and every other edge left
/// alone.
///
/// RE-DERIVING IT COSTS CLOSE TO A MINUTE on a large map, because it means reading a large
/// database that was just written. A partial run holds only the rows of the files it
/// re-extracted, so the graph is repaired rather than rebuilt.
///
/// EVERY EDGE A REPLACED FILE'S ROWS MADE IS DROPPED AND RE-DERIVED, and every other edge is kept: it was
/// made by rows this run did not touch, so it still says what they point at. A kept row pointing at an id
/// the run replaced is refused before the write (`reads::dangling`), so a kept edge cannot point at nothing.
pub fn merge(
    previous: &IndexMap<String, Vec<String>>,
    tables: Tables<'_>,
    replaced: &[String],
) -> IndexMap<String, Vec<String>> {
    let gone: IndexSet<&str> = replaced.iter().map(String::as_str).collect();
    let mut out: IndexMap<String, Vec<String>> = IndexMap::new();
    for (target, dependents) in previous {
        let kept: Vec<String> = dependents
            .iter()
            .filter(|d| !gone.contains(d.as_str()))
            .cloned()
            .collect();
        if !kept.is_empty() {
            out.insert(target.clone(), kept);
        }
    }
    for (target, dependents) in graph(tables) {
        let merged = out.entry(target).or_default();
        merged.extend(dependents);
        merged.sort();
        merged.dedup();
    }
    out
}

/// The same repair for the scope set: the replaced files drop out, and whatever they declare now
/// goes back in.
pub fn merge_scope(previous: &[String], tables: Tables<'_>, replaced: &[String]) -> Vec<String> {
    let gone: IndexSet<&str> = replaced.iter().map(String::as_str).collect();
    let mut out: IndexSet<String> = previous
        .iter()
        .filter(|p| !gone.contains(p.as_str()))
        .cloned()
        .collect();
    out.extend(scope_files(tables));
    let mut paths: Vec<String> = out.into_iter().collect();
    paths.sort();
    paths
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use serde_json::json;

    fn tables(value: Value) -> Map<String, Value> {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn a_row_pointing_at_another_files_row_is_an_edge_between_the_two_files() {
        let t = tables(json!({
            "files": [{"id": "f:1", "path": "a.ts"}, {"id": "f:2", "path": "b.ts"}],
            "classes": [{"id": "c:1", "file": "f:2"}],
            "calls": [{"id": "cl:1", "file": "f:1", "target_id": "c:1"}]
        }));
        // b.ts changing means a.ts has to be read again: its call names a row in b.ts.
        assert_eq!(graph(&t).get("b.ts"), Some(&vec!["a.ts".to_string()]));
    }

    #[test]
    fn a_template_naming_its_component_is_found_without_anyone_listing_the_relation() {
        // The hand-written graph missed exactly this: an `.html` row names a class in a `.ts`,
        // and neither file imports the other.
        let t = tables(json!({
            "files": [{"id": "f:1", "path": "a.html"}, {"id": "f:2", "path": "a.ts"}],
            "classes": [{"id": "c:1", "file": "f:2"}],
            "templates": [{"id": "t:1", "file": "f:1", "class": "c:1"}]
        }));
        assert_eq!(graph(&t).get("a.ts"), Some(&vec!["a.html".to_string()]));
    }

    #[test]
    fn a_changed_template_re_extracts_the_component_that_owns_it() {
        // The template pass runs FROM the component, so this edge goes the other way from every
        // other one: the `.html` depends on the `.ts` by reference, and the `.ts` has to be re-run
        // for the `.html` to be extracted at all.
        let t = tables(json!({
            "files": [{"id": "f:1", "path": "a.html"}, {"id": "f:2", "path": "a.ts"}],
            "classes": [{"id": "c:1", "file": "f:2"}],
            "templates": [{"id": "t:1", "file": "f:1", "class": "c:1"}]
        }));
        let g = graph(&t);
        assert_eq!(g.get("a.html"), Some(&vec!["a.ts".to_string()]), "changing the template");
        assert_eq!(g.get("a.ts"), Some(&vec!["a.html".to_string()]), "and changing the component");
    }

    #[test]
    fn an_import_names_a_file_and_not_a_row_in_one() {
        let t = tables(json!({
            "files": [{"id": "f:1", "path": "a.ts"}, {"id": "f:2", "path": "b.ts"}],
            "imports": [{"id": "i:1", "file": "f:1", "resolved_file": "f:2"}]
        }));
        assert_eq!(graph(&t).get("b.ts"), Some(&vec!["a.ts".to_string()]));
    }

    #[test]
    fn a_string_that_merely_holds_a_colon_is_not_an_id() {
        assert!(looks_like_id("b:4711"));
        assert!(!looks_like_id("http://example.com"));
        assert!(!looks_like_id("name:"));
        assert!(!looks_like_id(":4711"));
        assert!(!looks_like_id("toolong:1"), "the prefix is bounded");
        assert!(!looks_like_id("b:x"));
    }

    #[test]
    fn a_row_pointing_INSIDE_its_own_file_is_not_an_edge() {
        let t = tables(json!({
            "files": [{"id": "f:1", "path": "a.ts"}],
            "classes": [{"id": "c:1", "file": "f:1"}],
            "calls": [{"id": "cl:1", "file": "f:1", "target_id": "c:1"}]
        }));
        assert!(graph(&t).is_empty());
    }

    #[test]
    fn a_file_the_files_table_does_not_name_claims_no_edge() {
        // A row pointing at a file with no path is one the reader could not act on anyway.
        let t = tables(json!({
            "files": [{"id": "f:1", "path": "a.ts"}],
            "classes": [{"id": "c:1", "file": "f:9"}],
            "calls": [{"id": "cl:1", "file": "f:1", "target_id": "c:1"}]
        }));
        assert!(graph(&t).is_empty());
    }

    #[test]
    fn the_scope_is_every_file_declaring_something_a_template_names() {
        let t = tables(json!({
            "files": [{"id": "f:1", "path": "a.ts"}, {"id": "f:2", "path": "b.ts"},
                      {"id": "f:3", "path": "c.ts"}],
            "selector_index": [{"id": "si:1", "file": "f:1"}],
            "pipes": [{"id": "p:1", "file": "f:2"}],
            "calls": [{"id": "cl:1", "file": "f:3"}]
        }));
        assert_eq!(scope_files(&t), vec!["a.ts".to_string(), "b.ts".to_string()]);
    }

    #[test]
    fn a_repair_drops_every_edge_out_of_a_replaced_file_and_keeps_the_rest() {
        let mut previous: IndexMap<String, Vec<String>> = IndexMap::new();
        previous.insert("b.ts".into(), vec!["a.ts".into(), "z.ts".into()]);
        previous.insert("q.ts".into(), vec!["a.ts".into()]);
        let t = tables(json!({
            "files": [{"id": "f:1", "path": "a.ts"}, {"id": "f:2", "path": "b.ts"}],
            "classes": [{"id": "c:1", "file": "f:2"}],
            "calls": [{"id": "cl:1", "file": "f:1", "target_id": "c:1"}]
        }));
        let merged = merge(&previous, &t, &["a.ts".to_string()]);
        // `a.ts` was re-extracted, so its edges are re-derived and the one it no longer has is gone.
        assert_eq!(merged.get("b.ts"), Some(&vec!["a.ts".to_string(), "z.ts".to_string()]));
        assert_eq!(merged.get("q.ts"), None, "nothing points at q.ts any more");
    }

    #[test]
    fn a_repair_of_the_scope_set_does_the_same() {
        let t = tables(json!({
            "files": [{"id": "f:1", "path": "a.ts"}],
            "pipes": [{"id": "p:1", "file": "f:1"}]
        }));
        let out = merge_scope(&["a.ts".to_string(), "gone.ts".to_string()], &t, &["gone.ts".to_string()]);
        assert_eq!(out, vec!["a.ts".to_string()]);
    }
}
