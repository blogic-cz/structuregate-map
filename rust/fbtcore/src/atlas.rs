//! ONE NAVIGABLE OVERVIEW OF A FINISHED MAP, written beside it. A SECOND ENTRY POINT.
//!
//! The database says how many rows each table has; it does not say what the APPLICATION is. Orienting
//! in a million rows means opening tables until a shape appears. This answers the first three questions
//! a reader actually has - which projects exist, what each is made of, and where a screen lives - and
//! every number is a JOIN over already-published rows. It EXTRACTS NOTHING and parses no source.
//!
//! Two views of one model: `atlas.json` keeps ids so it joins back, `atlas.md` is the readable one.
//!
//! PARTITIONING IS THE WORKSPACE'S OWN, NOT A DIRECTORY DEPTH. An "area" is an NgModule - the unit the
//! frontend itself declares components into. Slicing by path segment needs a depth constant and invents
//! a hierarchy Angular does not have. Components in no module are counted separately, not folded into
//! a guess.
//!
//! REACH IS A CYCLE-SAFE WALK of `render_graph`, never a depth budget. These graphs contain cycles - a
//! wrapper rendering a child that renders the wrapper's sibling - and a visited set terminates where a
//! depth limit silently truncates and reports a smaller application than exists.

use anyhow::Result;
use indexmap::{IndexMap, IndexSet};
use rusqlite::Connection;
use serde_json::{Map, Value};
use std::path::Path;

const HALF: &str = "typescript";

type Row = Map<String, Value>;

/// What the build did, for the note the half prints.
pub struct Written {
    pub projects: usize,
    pub routes: usize,
    pub boundaries: usize,
    pub skipped: Option<String>,
}

#[path = "atlas_model.rs"]
mod model;
use model::*;

pub fn build(db: &Connection) -> Value {
    let meta = meta_of(db);
    let spec: Value = meta
        .get(&format!("spec:{HALF}"))
        .and_then(|t| serde_json::from_str(t).ok())
        .unwrap_or(Value::Null);
    let json_of = |table: &str| -> IndexSet<String> {
        spec.get("json_columns")
            .and_then(|c| c.get(table))
            .and_then(Value::as_array)
            .map(|c| c.iter().filter_map(Value::as_str).map(str::to_string).collect())
            .unwrap_or_default()
    };
    let table = |name: &str| rows(db, name, &json_of(name));

    let files = table("files");
    let mut file_by_id = IndexMap::new();
    let mut file_by_abs = IndexMap::new();
    for f in &files {
        if let Some(id) = text(f, "id") {
            if let Some(abs) = text(f, "abs") {
                file_by_abs.insert(abs, id.clone());
            }
            file_by_id.insert(id, f.clone());
        }
    }

    let classes = table("classes");
    let mut class_by_id = IndexMap::new();
    let mut class_by_file_name = IndexMap::new();
    for c in &classes {
        if let Some(id) = text(c, "id") {
            let file = text(c, "file").unwrap_or_default();
            let name = text(c, "name").unwrap_or_default();
            class_by_file_name.insert(format!("{file} {name}"), id.clone());
            class_by_id.insert(id, c.clone());
        }
    }

    let projects = table("projects");
    let mut by_dir_length = projects.clone();
    by_dir_length.sort_by_key(|p| std::cmp::Reverse(text(p, "dir").unwrap_or_default().len()));

    let components = table("components");
    let mut component_classes = IndexSet::new();
    let mut class_of_component: IndexMap<String, String> = IndexMap::new();
    for c in &components {
        if let Some(class) = text(c, "class") {
            component_classes.insert(class.clone());
            if let Some(id) = text(c, "id") {
                class_of_component.insert(id, class);
            }
        }
    }

    let templates = table("templates");
    let mut templates_of_class: IndexMap<String, usize> = IndexMap::new();
    let mut class_of_template: IndexMap<String, String> = IndexMap::new();
    for t in &templates {
        let Some(class) = text(t, "class") else { continue };
        if let Some(id) = text(t, "id") {
            class_of_template.insert(id, class.clone());
        }
        *templates_of_class.entry(class).or_insert(0) += 1;
    }

    // Gates hang off a COMPONENT; counted per class so every rollup joins on one key.
    let gates = table("gates");
    let mut gates_of_class: IndexMap<String, usize> = IndexMap::new();
    for g in &gates {
        let Some(component) = text(g, "component") else { continue };
        let Some(class) = class_of_component.get(&component) else { continue };
        *gates_of_class.entry(class.clone()).or_insert(0) += 1;
    }

    // A dynamic key has no `key` and is deliberately not counted as one.
    let mut keys_of_class: IndexMap<String, IndexSet<String>> = IndexMap::new();
    for r in table("i18n_refs") {
        let Some(template) = text(&r, "template") else { continue };
        let Some(class) = class_of_template.get(&template) else { continue };
        let Some(key) = text(&r, "key").filter(|k| !k.is_empty()) else { continue };
        keys_of_class.entry(class.clone()).or_default().insert(key);
    }

    let mut edges: IndexMap<String, Vec<String>> = IndexMap::new();
    for e in table("render_graph") {
        if let (Some(a), Some(b)) = (text(&e, "from_class"), text(&e, "to_class")) {
            edges.entry(a).or_default().push(b);
        }
    }

    let model = Model {
        file_by_id,
        file_by_abs,
        class_by_id,
        class_by_file_name,
        projects: projects.clone(),
        by_dir_length,
        component_classes,
        templates_of_class,
        gates_of_class,
        keys_of_class,
        edges,
    };

    // ---- areas = NgModules -------------------------------------------------------------------
    let mut areas_by_project: IndexMap<String, Vec<Row>> = IndexMap::new();
    let mut bootstrap_by_project: IndexMap<String, Vec<Value>> = IndexMap::new();
    let mut classes_in_some_module: IndexSet<String> = IndexSet::new();
    for m in table("ng_modules") {
        let declared: Vec<String> = m
            .get("declarations")
            .and_then(Value::as_array)
            .map(|d| d.iter().filter_map(|one| model.class_of_ref(Some(one))).collect())
            .unwrap_or_default();
        classes_in_some_module.extend(declared.iter().cloned());
        let Some(project) = model.project_of_file(text(&m, "file").as_deref()) else { continue };

        let mut area = Row::new();
        area.insert("module".into(), Value::String(text(&m, "id").unwrap_or_default()));
        area.insert("name".into(), Value::String(text(&m, "name").unwrap_or_else(|| "None".into())));
        let dir = model.path_of_file(text(&m, "file").as_deref()).unwrap_or_default();
        area.insert("dir".into(), Value::String(dir_of(&dir)));
        model.sum_over(&declared).into_value(&mut area);
        areas_by_project.entry(project.clone()).or_default().push(area);

        for b in m.get("bootstrap").and_then(Value::as_array).cloned().unwrap_or_default() {
            let mut one = Row::new();
            one.insert("name".into(), b.get("name").cloned().unwrap_or(Value::Null));
            let class = model.class_of_ref(Some(&b));
            one.insert(
                "file".into(),
                model
                    .path_of_class(class.as_deref())
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            );
            bootstrap_by_project.entry(project.clone()).or_default().push(Value::Object(one));
        }
    }

    // ---- routes and what they reach ----------------------------------------------------------
    let mut routes_by_project: IndexMap<String, Vec<Row>> = IndexMap::new();
    let mut boundaries: Vec<Value> = Vec::new();
    let routes = table("routes");
    for r in &routes {
        let class = model.class_of_ref(r.get("component"));
        let lazy_source = r
            .get("load_children")
            .filter(|v| !v.is_null())
            .or_else(|| r.get("load_component").filter(|v| !v.is_null()));
        let lazy = lazy_source
            .and_then(|v| v.get("$fn"))
            .and_then(Value::as_str)
            .map(str::to_string);
        // A route's own project is where its route TABLE lives; the component it names may sit
        // elsewhere, and when it does the crossing is the interesting fact rather than an error.
        let home = model.project_of_file(text(r, "file").as_deref());
        let component_name = r
            .get("component")
            .and_then(Value::as_object)
            .and_then(|c| c.get("name"))
            .cloned()
            .unwrap_or(Value::Null);

        let mut route = Row::new();
        route.insert("full_path".into(), r.get("full_path").cloned().unwrap_or(Value::Null));
        route.insert("component".into(), component_name.clone());
        route.insert(
            "component_class".into(),
            class.clone().map(Value::String).unwrap_or(Value::Null),
        );
        route.insert(
            "component_file".into(),
            model.path_of_class(class.as_deref()).map(Value::String).unwrap_or(Value::Null),
        );
        route.insert(
            "crosses_project".into(),
            Value::Bool(class.is_some() && model.project_of_class(class.as_deref()) != home),
        );
        let guards = r.get("guards").and_then(Value::as_array).map(Vec::len).unwrap_or(0)
            + r.get("can_activate").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
        route.insert("guards".into(), Value::from(guards));
        // THE COUNT, WHICH AN "AS A LIST" READ REPORTS AS 0 FOR EVERY ROUTE - a helper that reads this
        // column as a list and takes the length gets nothing, but `routes.children` is a NUMBER.
        // A deliberate divergence from a plain row copy: the atlas is a document meant to be
        // read, and a count nobody can act on is worse than none.
        route.insert(
            "children".into(),
            match r.get("children") {
                Some(Value::Number(c)) if c.as_i64() != Some(0) => Value::Number(c.clone()),
                _ => Value::from(0),
            },
        );
        route.insert("lazy".into(), lazy.clone().map(Value::String).unwrap_or(Value::Null));
        let reach = class.as_deref().map(|c| model.reach_from(c));
        route.insert(
            "reach".into(),
            match reach.clone() {
                Some(made) => {
                    let mut into = Row::new();
                    made.into_value(&mut into);
                    Value::Object(into)
                }
                None => Value::Null,
            },
        );

        // A LAZY ROUTE IS ONLY A BOUNDARY WHILE ITS TARGET IS UNRESOLVED. A RECOVERY TARGET LEAVES
        // THE BOUNDARY STANDING: where the only import the compiler could follow sits in a rejection
        // handler, the real destination is still unresolved. SQLITE HAS NO BOOLEAN - `true` is stored
        // as 1, so this reads the column as TRUTHY, which is what it means.
        let truthy = |v: Option<&Value>| match v {
            None | Some(Value::Null) => false,
            Some(Value::Bool(b)) => *b,
            Some(Value::Number(x)) => x.as_i64() != Some(0),
            Some(Value::String(s)) => !s.is_empty(),
            Some(_) => true,
        };
        let resolved = (r.get("load_children_module").is_some_and(|v| !v.is_null())
            || r.get("load_component_id").is_some_and(|v| !v.is_null()))
            && !truthy(r.get("load_children_recovery"));

        if lazy.is_some() && !resolved {
            let mut one = Row::new();
            one.insert(
                "project".into(),
                model.project_name(home.as_deref()).map(Value::String).unwrap_or(Value::Null),
            );
            one.insert("full_path".into(), route["full_path"].clone());
            one.insert("kind".into(), Value::String("lazy_import".into()));
            one.insert("target".into(), Value::String(lazy.clone().unwrap_or_default()));
            boundaries.push(Value::Object(one));
        } else if class.is_none() && !component_name.is_null() {
            let mut one = Row::new();
            one.insert(
                "project".into(),
                model.project_name(home.as_deref()).map(Value::String).unwrap_or(Value::Null),
            );
            one.insert("full_path".into(), route["full_path"].clone());
            one.insert("kind".into(), Value::String("component_outside_map".into()));
            one.insert("target".into(), component_name.clone());
            boundaries.push(Value::Object(one));
        }

        if let Some(project) = model.project_of_class(class.as_deref()).or(home) {
            routes_by_project.entry(project).or_default().push(route);
        }
    }

    // ---- per project -------------------------------------------------------------------------
    let mut classes_by_project: IndexMap<String, Vec<String>> = IndexMap::new();
    for c in &classes {
        let Some(project) = model.project_of_file(text(c, "file").as_deref()) else { continue };
        if let Some(id) = text(c, "id") {
            classes_by_project.entry(project).or_default().push(id);
        }
    }

    let mut atlas_projects: Vec<Row> = Vec::new();
    for p in &model.projects {
        let id = text(p, "id").unwrap_or_default();
        let mut by_ext: IndexMap<String, usize> = IndexMap::new();
        for f in &files {
            if model.project_of_file(text(f, "id").as_deref()).as_deref() == Some(id.as_str()) {
                let ext = match f.get("ext") {
                    Some(Value::String(e)) => e.clone(),
                    Some(Value::Null) | None => "None".to_string(),
                    Some(other) => other.to_string(),
                };
                *by_ext.entry(ext).or_insert(0) += 1;
            }
        }
        let mut by_count: Vec<(String, usize)> = by_ext.into_iter().collect();
        by_count.sort_by_key(|(_, c)| std::cmp::Reverse(*c));

        let own = classes_by_project.get(&id).cloned().unwrap_or_default();
        let mut areas = areas_by_project.get(&id).cloned().unwrap_or_default();
        areas.sort_by_key(|a| {
            std::cmp::Reverse(a.get("components").and_then(Value::as_i64).unwrap_or(0))
        });
        let mut project_routes = routes_by_project.get(&id).cloned().unwrap_or_default();
        project_routes.sort_by_key(|r| {
            std::cmp::Reverse(
                r.get("reach")
                    .and_then(|v| v.get("components"))
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
            )
        });

        let mut entry = Row::new();
        entry.insert("id".into(), Value::String(id.clone()));
        entry.insert("name".into(), Value::String(text(p, "name").unwrap_or_else(|| "None".into())));
        entry.insert("dir".into(), Value::String(text(p, "dir").unwrap_or_else(|| "None".into())));
        entry.insert("project_type".into(), p.get("project_type").cloned().unwrap_or(Value::Null));
        let mut files_map = Row::new();
        for (ext, count) in by_count {
            files_map.insert(ext, Value::from(count));
        }
        entry.insert("files".into(), Value::Object(files_map));
        model.sum_over(&own).into_value(&mut entry);
        entry.insert("ng_modules".into(), Value::from(areas.len()));
        entry.insert(
            "components_in_no_module".into(),
            Value::from(
                own.iter()
                    .filter(|c| {
                        model.component_classes.contains(*c) && !classes_in_some_module.contains(*c)
                    })
                    .count(),
            ),
        );
        entry.insert(
            "bootstrap".into(),
            Value::Array(bootstrap_by_project.get(&id).cloned().unwrap_or_default()),
        );
        entry.insert(
            "areas".into(),
            Value::Array(areas.into_iter().map(Value::Object).collect()),
        );
        entry.insert(
            "routes".into(),
            Value::Array(project_routes.into_iter().map(Value::Object).collect()),
        );
        atlas_projects.push(entry);
    }
    atlas_projects.sort_by_key(|p| {
        std::cmp::Reverse(p.get("components").and_then(Value::as_i64).unwrap_or(0))
    });

    let options: Value = meta
        .get(&format!("options:{HALF}"))
        .and_then(|t| serde_json::from_str(t).ok())
        .unwrap_or(Value::Object(Map::new()));

    let mut scope = Row::new();
    scope.insert("all".into(), Value::Bool(true));
    let mut from = Row::new();
    from.insert("scope".into(), Value::Object(scope));
    from.insert(
        "source_revision".into(),
        meta.get(&format!("source_revision:{HALF}"))
            .and_then(|t| serde_json::from_str(t).ok())
            .unwrap_or(Value::Null),
    );
    from.insert(
        "generated_at_ms".into(),
        Value::from(
            meta.get(&format!("generated_at_ms:{HALF}"))
                .and_then(|t| t.parse::<i64>().ok())
                .unwrap_or(0),
        ),
    );

    let mut totals = Row::new();
    totals.insert("projects".into(), Value::from(atlas_projects.len()));
    totals.insert("components".into(), Value::from(components.len()));
    totals.insert("templates".into(), Value::from(templates.len()));
    totals.insert("routes".into(), Value::from(routes.len()));
    totals.insert("gates".into(), Value::from(gates.len()));

    let mut atlas = Row::new();
    atlas.insert("generated_from".into(), Value::Object(from));
    atlas.insert(
        "project_docs".into(),
        match options.get("project_docs") {
            Some(Value::String(s)) => Value::String(s.clone()),
            _ => Value::Null,
        },
    );
    atlas.insert(
        "projects".into(),
        Value::Array(atlas_projects.into_iter().map(Value::Object).collect()),
    );
    atlas.insert("boundaries".into(), Value::Array(boundaries));
    atlas.insert("totals".into(), Value::Object(totals));
    Value::Object(atlas)
}

#[path = "atlas_markdown.rs"]
mod page;
use page::*;

pub fn write(db_path: &Path, into: &Path) -> Result<Written> {
    if !db_path.exists() {
        return Ok(Written {
            projects: 0,
            routes: 0,
            boundaries: 0,
            skipped: Some(format!("no database at {}", db_path.display())),
        });
    }
    let db = Connection::open(db_path)?;
    let atlas = build(&db);
    drop(db);

    let projects = atlas.get("projects").and_then(Value::as_array).map(Vec::len).unwrap_or(0);
    if projects == 0 {
        return Ok(Written {
            projects: 0,
            routes: 0,
            boundaries: 0,
            skipped: Some("the database holds no Angular projects".to_string()),
        });
    }
    std::fs::create_dir_all(into)?;
    std::fs::write(into.join("atlas.json"), pretty(&atlas))?;
    std::fs::write(into.join("atlas.md"), markdown(&atlas))?;
    Ok(Written {
        projects: number(atlas.pointer("/totals/projects")) as usize,
        routes: number(atlas.pointer("/totals/routes")) as usize,
        boundaries: atlas.get("boundaries").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        skipped: None,
    })
}
