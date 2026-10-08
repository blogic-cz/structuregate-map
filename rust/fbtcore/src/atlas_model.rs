//! The rows the atlas is built from: read, indexed, and what each class is made of.
//! A child of `atlas` by `#[path]`: what it holds is `pub(super)`, for that file alone.

use super::*;

/// Every row of one table that this half wrote, with its structure columns decoded.
pub(super) fn rows(db: &Connection, table: &str, json_columns: &IndexSet<String>) -> Vec<Row> {
    let sql = format!("SELECT * FROM \"{}\" WHERE half = ?1", table.replace('"', "\"\""));
    let Ok(mut stmt) = db.prepare(&sql) else { return Vec::new() };
    let names: Vec<String> = stmt.column_names().iter().map(|c| (*c).to_string()).collect();
    let Ok(mut found) = stmt.query([HALF]) else { return Vec::new() };
    let mut out = Vec::new();
    while let Ok(Some(record)) = found.next() {
        let mut row = Map::new();
        for (index, name) in names.iter().enumerate() {
            let raw: rusqlite::types::Value = match record.get(index) {
                Ok(v) => v,
                Err(_) => rusqlite::types::Value::Null,
            };
            let mut value = match raw {
                rusqlite::types::Value::Null => Value::Null,
                rusqlite::types::Value::Integer(i) => Value::from(i),
                rusqlite::types::Value::Real(f) => Value::from(f),
                rusqlite::types::Value::Text(t) => Value::String(t),
                rusqlite::types::Value::Blob(b) => Value::String(String::from_utf8_lossy(&b).into_owned()),
            };
            if !value.is_null()
                && json_columns.contains(name)
                && let Value::String(text) = &value
                && let Ok(decoded) = serde_json::from_str::<Value>(text)
            {
                value = decoded;
            }
            row.insert(name.clone(), value);
        }
        out.push(row);
    }
    out
}

pub(super) fn text(row: &Row, key: &str) -> Option<String> {
    row.get(key).and_then(Value::as_str).map(str::to_string)
}

pub(super) fn dir_of(path: &str) -> String {
    match path.rfind('/') {
        Some(at) => path[..at + 1].to_string(),
        None => String::new(),
    }
}

/// Thin spaces between thousands, so a six-digit count is readable in a table cell.
pub(super) fn n(value: i64) -> String {
    let digits = value.abs().to_string();
    let mut grouped = String::new();
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            grouped.push(' ');
        }
        grouped.push(ch);
    }
    if value < 0 { format!("-{grouped}") } else { grouped }
}

/// A cell must not contain the row separator, and a lazy import's source text spans lines.
pub(super) fn cell(value: &str) -> String {
    value.replace(['\r', '\n'], " ").replace('|', "\\|")
}

/// What one class, or a set of them, is made of.
#[derive(Default, Clone)]
pub(super) struct Made {
    pub(super) components: usize,
    pub(super) templates: usize,
    pub(super) gates: usize,
    pub(super) keys: usize,
}

impl Made {
    pub(super) fn into_value(self, into: &mut Row) {
        into.insert("components".into(), Value::from(self.components));
        into.insert("templates".into(), Value::from(self.templates));
        into.insert("gates".into(), Value::from(self.gates));
        into.insert("i18n_keys".into(), Value::from(self.keys));
    }
}

pub(super) struct Model {
    pub(super) file_by_id: IndexMap<String, Row>,
    pub(super) file_by_abs: IndexMap<String, String>,
    pub(super) class_by_id: IndexMap<String, Row>,
    pub(super) class_by_file_name: IndexMap<String, String>,
    pub(super) projects: Vec<Row>,
    pub(super) by_dir_length: Vec<Row>,
    pub(super) component_classes: IndexSet<String>,
    pub(super) templates_of_class: IndexMap<String, usize>,
    pub(super) gates_of_class: IndexMap<String, usize>,
    pub(super) keys_of_class: IndexMap<String, IndexSet<String>>,
    pub(super) edges: IndexMap<String, Vec<String>>,
}

impl Model {
    pub(super) fn path_of_file(&self, id: Option<&str>) -> Option<String> {
        self.file_by_id.get(id?).and_then(|f| text(f, "path"))
    }

    pub(super) fn path_of_class(&self, id: Option<&str>) -> Option<String> {
        let file = self.class_by_id.get(id?).and_then(|c| text(c, "file"))?;
        self.path_of_file(Some(&file))
    }

    /// A reference to a declaration, as `{file, name}`, resolved to a class of this map.
    ///
    /// A class is keyed by FILE + NAME, never by name alone - two `AppModule`s and two
    /// same-named components answer to the same name in one monorepo.
    pub(super) fn class_of_ref(&self, reference: Option<&Value>) -> Option<String> {
        let reference = reference?.as_object()?;
        let abs = reference.get("file")?.as_str()?;
        let name = reference.get("name")?.as_str()?;
        let file = self.file_by_abs.get(abs)?;
        self.class_by_file_name.get(&format!("{file} {name}")).cloned()
    }

    /// WHERE A FILE LIVES BEATS WHICH PROGRAM REACHED IT FIRST. `files.project` records the program
    /// that parsed a file, and several programs contain the same shared sources - whichever ran first
    /// claimed them, so a library reported 0 components while its own were counted under the
    /// application. The project's OWN published `dir` is the stable answer, longest match first so a
    /// nested project cannot be swallowed by a shorter root.
    pub(super) fn project_of_path(&self, path: Option<&str>) -> Option<String> {
        let path = path?;
        for project in &self.by_dir_length {
            let dir = text(project, "dir").unwrap_or_default();
            if path.starts_with(&format!("{dir}/")) {
                return text(project, "id");
            }
        }
        None
    }

    pub(super) fn project_of_file(&self, id: Option<&str>) -> Option<String> {
        let file = self.file_by_id.get(id?)?;
        let path = text(file, "path");
        self.project_of_path(path.as_deref()).or_else(|| text(file, "project"))
    }

    pub(super) fn project_of_class(&self, id: Option<&str>) -> Option<String> {
        let file = self.class_by_id.get(id?).and_then(|c| text(c, "file"))?;
        self.project_of_file(Some(&file))
    }

    pub(super) fn project_name(&self, id: Option<&str>) -> Option<String> {
        let id = id?;
        self.projects
            .iter()
            .find(|p| text(p, "id").as_deref() == Some(id))
            .and_then(|p| text(p, "name"))
    }

    /// Everything a class can render, transitively. Cycle-safe by visited set.
    pub(super) fn reach_from(&self, root: &str) -> Made {
        let mut seen: IndexSet<String> = IndexSet::new();
        seen.insert(root.to_string());
        let mut queue = vec![root.to_string()];
        let (mut templates, mut gates) = (0usize, 0usize);
        let mut keys: IndexSet<String> = IndexSet::new();
        while let Some(current) = queue.pop() {
            templates += self.templates_of_class.get(&current).copied().unwrap_or(0);
            gates += self.gates_of_class.get(&current).copied().unwrap_or(0);
            if let Some(theirs) = self.keys_of_class.get(&current) {
                keys.extend(theirs.iter().cloned());
            }
            if let Some(next) = self.edges.get(&current) {
                for target in next {
                    if seen.insert(target.clone()) {
                        queue.push(target.clone());
                    }
                }
            }
        }
        Made { components: seen.len(), templates, gates, keys: keys.len() }
    }

    pub(super) fn sum_over(&self, class_ids: &[String]) -> Made {
        let (mut components, mut templates, mut gates) = (0usize, 0usize, 0usize);
        let mut keys: IndexSet<String> = IndexSet::new();
        for id in class_ids {
            if self.component_classes.contains(id) {
                components += 1;
            }
            templates += self.templates_of_class.get(id).copied().unwrap_or(0);
            gates += self.gates_of_class.get(id).copied().unwrap_or(0);
            if let Some(theirs) = self.keys_of_class.get(id) {
                keys.extend(theirs.iter().cloned());
            }
        }
        Made { components, templates, gates, keys: keys.len() }
    }
}

pub(super) fn meta_of(db: &Connection) -> IndexMap<String, String> {
    let mut out = IndexMap::new();
    let Ok(mut stmt) = db.prepare("SELECT key, value FROM _meta") else { return out };
    let Ok(found) = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
    else {
        return out;
    };
    for pair in found.flatten() {
        out.insert(pair.0, pair.1);
    }
    out
}
