//! WHAT THE GRAPH AND THE CHECKS NEED FROM A PAYLOAD, gathered one row at a time.
//!
//! A large workspace's payload is gigabytes of rows once parsed, and holding them all to derive a few maps from
//! was most of this half's peak memory. So the rows are streamed twice past a `Digest` and never held: the FIRST
//! pass takes what has to be known before any row can be judged - every file's path and reads, the file each id
//! belongs to, every id there is, the facts the template/import/barrel edges are built from - and the SECOND, run
//! while the rows are written, makes each row's own edges and checks its references against its owner's reads.
//!
//! Both passes visit the rows in payload order, the order the old whole-payload walks used, so the graph and the
//! findings come out identical - `deps::graph`, `deps::scope_files` and `reads::unread` are this, run over a held
//! map (`Digest::over`).

use indexmap::{IndexMap, IndexSet};
use serde_json::{Map, Value};

type Row = Map<String, Value>;

/// The columns that never hold a reference to another row.
const SKIP_COLUMNS: &[&str] = &["id", "file", "half"];

/// The tables that say "this file declares something a template can name".
const SCOPE_TABLES: &[&str] = &["selector_index", "pipes", "directives", "ng_modules"];

/// `b:4711` - a short alphabetic prefix, a colon, digits. The shape every map handle has.
pub(super) fn looks_like_id(value: &str) -> bool {
    let Some(cut) = value.find(':') else { return false };
    if cut == 0 || cut > 3 || cut == value.len() - 1 {
        return false;
    }
    value[..cut].bytes().all(|b| b.is_ascii_alphabetic()) && value[cut + 1..].bytes().all(|b| b.is_ascii_digit())
}

fn text(row: &Row, name: &str) -> Option<String> {
    match row.get(name) {
        Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
        _ => None,
    }
}

/// Every string in `value` shaped like one of node's ids (`isId` in `TsStore.mjs`: one to four lowercase letters, a
/// colon, digits) into `into`, under `label` unless already there. A walk over the value's own nesting, no bound.
fn name_ids(value: &Value, label: &mut dyn FnMut() -> String, into: &mut IndexMap<String, String>) {
    match value {
        Value::String(s) => {
            let shaped = s.find(':').is_some_and(|cut| {
                (1..=4).contains(&cut) && cut < s.len() - 1 && s[..cut].bytes().all(|b| b.is_ascii_lowercase()) && s[cut + 1..].bytes().all(|b| b.is_ascii_digit())
            });
            if shaped && !into.contains_key(s) {
                into.insert(s.clone(), label());
            }
        }
        Value::Array(items) => items.iter().for_each(|v| name_ids(v, label, into)),
        Value::Object(map) => map.values().for_each(|v| name_ids(v, label, into)),
        _ => {}
    }
}

#[derive(Default)]
pub struct Digest {
    /// Tables a rollup rebuilds whole: their rows are derived again, so a reference in them proves nothing.
    rebuilt: Vec<String>,
    /// Every `files` row, whole - small, and what the source index is read from.
    pub files: Vec<Value>,
    path_of: IndexMap<String, String>,
    reads: IndexMap<String, IndexSet<String>>,
    /// id -> the `file` its row names (the graph's owner).
    owner: IndexMap<String, String>,
    /// id -> the file that EXTRACTED its row (`owner_file`, else `file`; a `files` row owns itself).
    owner_of: IndexMap<String, String>,
    /// Every id the payload holds - what a partial run's kept rows may still point at (`dangling`).
    pub ids: IndexSet<String>,
    scope_ids: IndexSet<String>,
    templates: Vec<(Option<String>, Option<String>)>,
    imports: Vec<(String, String)>,
    names: Vec<(String, Option<String>, Vec<String>)>,
    /// The second pass's: target file id -> the file ids whose rows point into it.
    out: IndexMap<String, IndexSet<String>>,
    unread: IndexSet<(String, String)>,
    scope_paths: Option<IndexSet<String>>,
    /// The tables whose rows `named` is read from - the whole ones, on a run that stopped short; none otherwise.
    naming: IndexSet<String>,
    /// Every id a row of those tables holds, however deeply nested -> `table.column` of the first that does
    /// (`dangling::dangling`): the rows the map will hold in place of the ones the replace deletes.
    pub named: IndexMap<String, String>,
    /// An id a row points at that no payload row holds -> the files whose rows point at it: on a partial run a row it KEPT,
    /// whose file the payload does not say (`resolve_kept`).
    pending: IndexMap<String, IndexSet<String>>,
}

impl Digest {
    pub fn new(rebuilt: &[String]) -> Digest {
        Digest { rebuilt: rebuilt.to_vec(), ..Default::default() }
    }

    /// ...and the ids the rows of `tables` name, gathered as the first pass goes (`named`).
    pub fn naming(mut self, tables: &[String]) -> Digest {
        self.naming = tables.iter().cloned().collect();
        self
    }

    /// Both passes over rows already held - what a test and a small payload use.
    pub fn over(tables: &Map<String, Value>, rebuilt: &[String]) -> Digest {
        let mut digest = Digest::new(rebuilt);
        let rows = || tables.iter().flat_map(|(name, rows)| rows.as_array().into_iter().flatten().filter_map(Value::as_object).map(move |r| (name.as_str(), r)));
        for (name, row) in rows() {
            digest.first(name, row);
        }
        for (name, row) in rows() {
            digest.second(name, row);
        }
        digest
    }

    /// The FIRST pass: what every later judgement needs.
    pub fn first(&mut self, table: &str, row: &Row) {
        if table == "files" {
            self.files.push(Value::Object(row.clone()));
            if let (Some(id), Some(path)) = (text(row, "id"), text(row, "path")) {
                if row.contains_key("reads") {
                    self.reads.insert(path.clone(), super::deps::list(row.get("reads")).into_iter().collect());
                }
                self.path_of.insert(id, path);
            }
        }
        if let Some(Value::String(id)) = row.get("id") {
            self.ids.insert(id.clone());
        }
        if self.naming.contains(table) {
            for (column, value) in row {
                if column != "id" && column != "owner_file" && column != "half" {
                    name_ids(value, &mut || format!("{table}.{column}"), &mut self.named);
                }
            }
        }
        if let Some(id) = text(row, "id") {
            if let Some(file) = text(row, "file") {
                self.owner.insert(id.clone(), file);
            }
            let home = if table == "files" { Some(id.clone()) } else { text(row, "owner_file").or_else(|| text(row, "file")) };
            if let Some(home) = home {
                self.owner_of.insert(id, home);
            }
        }
        if SCOPE_TABLES.contains(&table)
            && let Some(file) = text(row, "file")
        {
            self.scope_ids.insert(file);
        }
        match table {
            "templates" => self.templates.push((text(row, "file"), text(row, "class"))),
            "imports" => {
                if let (Some(importer), Some(imported)) = (text(row, "file"), text(row, "resolved_file")) {
                    self.imports.push((importer, imported));
                }
            }
            "import_names" => {
                if let Some(importer) = text(row, "file") {
                    self.names.push((importer, text(row, "declared_file"), super::deps::list(row.get("via"))));
                }
            }
            _ => {}
        }
    }

    /// The SECOND pass, once the first has seen every row: the edges this row makes, and the references it holds
    /// that its owner never read.
    pub fn second(&mut self, table: &str, row: &Row) {
        if let Some(home) = text(row, "file") {
            for (column, value) in row {
                if SKIP_COLUMNS.contains(&column.as_str()) {
                    continue;
                }
                let Value::String(id) = value else { continue };
                if !looks_like_id(id) {
                    continue;
                }
                match self.owner.get(id) {
                    Some(target) if *target != home => {
                        self.out.entry(target.clone()).or_default().insert(home.clone());
                    }
                    Some(_) => {}
                    None => {
                        self.pending.entry(id.clone()).or_default().insert(home.clone());
                    }
                }
            }
        }
        if table == "files" || self.rebuilt.iter().any(|r| r == table) {
            return;
        }
        if self.scope_paths.is_none() {
            self.scope_paths = Some(self.scope_files().into_iter().collect());
        }
        let Some(own) = text(row, "owner_file").or_else(|| text(row, "file")).and_then(|o| self.path_of.get(&o).cloned()) else { return };
        let Some(read) = self.reads.get(&own) else { return };
        let template = |p: &str| p.ends_with(".html");
        if template(&own) {
            return;
        }
        let scope = self.scope_paths.as_ref().expect("made above");
        let answered = |p: &str| template(p) || scope.contains(p) || p.starts_with("node_modules/") || p.contains("/node_modules/");
        for (column, value) in row {
            if column == "id" || column == "owner_file" {
                continue;
            }
            let Value::String(v) = value else { continue };
            let Some(named) = self.owner_of.get(v).and_then(|o| self.path_of.get(o)) else { continue };
            if *named != own && !answered(named) && !read.contains(named) {
                self.unread.insert((own.clone(), named.clone()));
            }
        }
    }

    /// A PARTIAL RUN'S EDGES INTO THE ROWS IT KEPT: a re-extracted row pointing at a row of a file it did not read again
    /// (`this.modal.close()` at a kept method) made no edge - the payload does not hold that row, so it named no file - and
    /// the repaired graph lost the dependency a full run records. The file each such id's row names is read from the map.
    pub fn resolve_kept(&mut self, db: &rusqlite::Connection) {
        if self.pending.is_empty() {
            return;
        }
        let Ok(tables) = crate::rows::schema::listable_tables(db) else { return };
        let ids: Vec<String> = self.pending.keys().cloned().collect();
        for table in tables {
            let Ok(columns) = crate::rows::schema::existing_columns(db, &table) else { continue };
            if !columns.iter().any(|c| c == "id") || !columns.iter().any(|c| c == "file") {
                continue;
            }
            for chunk in ids.chunks(500) {
                let marks = vec!["?"; chunk.len()].join(",");
                let sql = format!("SELECT id, file FROM \"{}\" WHERE id IN ({marks}) AND file IS NOT NULL", table.replace('"', "\"\""));
                let Ok(mut stmt) = db.prepare(&sql) else { break };
                let Ok(rows) = stmt.query_map(rusqlite::params_from_iter(chunk.iter()), |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))) else { break };
                for (id, file) in rows.flatten() {
                    for home in self.pending.get(&id).into_iter().flatten() {
                        if *home != file {
                            self.out.entry(file.clone()).or_default().insert(home.clone());
                        }
                    }
                }
            }
        }
        self.pending.clear();
    }

    /// Which files depend on which, by path: the edges the rows made, then the template, import and barrel edges.
    pub fn graph(&self) -> IndexMap<String, Vec<String>> {
        let mut out = self.out.clone();
        // A TEMPLATE IS EXTRACTED BY WALKING ITS COMPONENT: a changed `.html` is re-extracted with the component that
        // owns it, so the `.html` depends on the `.ts` it names (see `deps::graph`).
        for (home, class) in &self.templates {
            let component = class.as_ref().and_then(|c| self.owner.get(c));
            if let (Some(home), Some(component)) = (home, component)
                && home != component
            {
                out.entry(home.clone()).or_default().insert(component.clone());
            }
        }
        // AN IMPORT NAMES A FILE DIRECTLY, not a row in one.
        for (importer, imported) in &self.imports {
            if importer != imported {
                out.entry(imported.clone()).or_default().insert(importer.clone());
            }
        }
        // A NAME IMPORTED THROUGH BARRELS depends on its declaring file AND every barrel on the way (`via`, paths).
        let id_of_path: IndexMap<&String, &String> = self.path_of.iter().map(|(id, path)| (path, id)).collect();
        for (importer, declared, via) in &self.names {
            let mut on_the_way: Vec<String> = declared.iter().cloned().collect();
            for path in via {
                if let Some(id) = id_of_path.get(path) {
                    on_the_way.push((*id).clone());
                }
            }
            for target in on_the_way {
                if target != *importer {
                    out.entry(target).or_default().insert(importer.clone());
                }
            }
        }
        // TO PATHS, dropping any file the `files` table does not name.
        let mut named: IndexMap<String, Vec<String>> = IndexMap::new();
        for (target, dependents) in out {
            let Some(target_path) = self.path_of.get(&target) else { continue };
            let mut paths: Vec<String> =
                dependents.iter().filter_map(|d| self.path_of.get(d)).filter(|p| *p != target_path).cloned().collect();
            if paths.is_empty() {
                continue;
            }
            paths.sort();
            paths.dedup();
            named.insert(target_path.clone(), paths);
        }
        named
    }

    /// Every file that declares something a template resolves by name, sorted.
    pub fn scope_files(&self) -> Vec<String> {
        let mut paths: Vec<String> = self.scope_ids.iter().filter_map(|f| self.path_of.get(f).cloned()).collect::<IndexSet<_>>().into_iter().collect();
        paths.sort();
        paths
    }

    /// What the rows name that the file extracting them never read, as `owner -> named` pairs.
    pub fn unread(&self) -> Vec<(String, String)> {
        self.unread.iter().cloned().collect()
    }
}
