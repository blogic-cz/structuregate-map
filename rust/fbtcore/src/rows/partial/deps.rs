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
/// A list cell's strings - decoded, or still the text a payload sent (see `rawcells`), decoded as deep as it goes.
pub(super) fn list(value: Option<&Value>) -> Vec<String> {
    let Some(value) = value else { return Vec::new() };
    let value = super::cells::decoded(value);
    value.as_array().into_iter().flatten().filter_map(|s| s.as_str().map(String::from)).collect()
}

/// Every table's rows, as the payload holds them.
pub type Tables<'a> = &'a Map<String, Value>;

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
    // ONE DERIVATION, streamed or held: the rows go past a `Digest` (`digest.rs`), which a run streaming its payload
    // feeds one row at a time.
    super::digest::Digest::over(tables, &[]).graph()
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
    super::digest::Digest::over(tables, &[]).scope_files()
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
/// the run replaced is refused before the replace (`dangling`), so a kept edge cannot point at nothing.
pub fn merge(
    previous: &IndexMap<String, Vec<String>>,
    fresh: IndexMap<String, Vec<String>>,
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
    for (target, dependents) in fresh {
        let merged = out.entry(target).or_default();
        merged.extend(dependents);
        merged.sort();
        merged.dedup();
    }
    out
}

/// The same repair for the scope set: the replaced files drop out, and whatever they declare now
/// goes back in.
pub fn merge_scope(previous: &[String], fresh: Vec<String>, replaced: &[String]) -> Vec<String> {
    let gone: IndexSet<&str> = replaced.iter().map(String::as_str).collect();
    let mut out: IndexSet<String> = previous
        .iter()
        .filter(|p| !gone.contains(p.as_str()))
        .cloned()
        .collect();
    out.extend(fresh);
    let mut paths: Vec<String> = out.into_iter().collect();
    paths.sort();
    paths
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use super::super::digest::looks_like_id;
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
        let merged = merge(&previous, graph(&t), &["a.ts".to_string()]);
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
        let out = merge_scope(&["a.ts".to_string(), "gone.ts".to_string()], scope_files(&t), &["gone.ts".to_string()]);
        assert_eq!(out, vec!["a.ts".to_string()]);
    }
}
