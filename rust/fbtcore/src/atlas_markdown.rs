//! The atlas as a page to read: the markdown written beside `atlas.json`.
//! A child of `atlas` by `#[path]`: what it holds is `pub(super)`, for that file alone.

use super::*;

pub(super) fn number(value: Option<&Value>) -> i64 {
    value.and_then(Value::as_i64).unwrap_or(0)
}

pub fn markdown(atlas: &Value) -> String {
    let mut out: Vec<String> = Vec::new();
    // `source_revision` is a RECORD (commit, branch, dirty), not a string - printing it raw put a JSON
    // blob in the first sentence a reader sees. The dirty flag stays: a map built from an uncommitted
    // tree is not the commit it names.
    let rev = atlas.pointer("/generated_from/source_revision");
    let revision = match rev {
        Some(Value::Object(r)) => {
            let short = r
                .get("short")
                .and_then(Value::as_str)
                .or_else(|| r.get("commit").and_then(Value::as_str))
                .map(str::to_string)
                .unwrap_or_else(|| "None".into());
            let branch = r.get("branch").and_then(Value::as_str).unwrap_or("None");
            let dirty = matches!(r.get("dirty"), Some(Value::Bool(true)));
            format!("{short} ({branch}){}", if dirty { " DIRTY" } else { "" })
        }
        Some(Value::Null) | None => "None".to_string(),
        Some(other) => other.to_string(),
    };
    let totals = atlas.get("totals").cloned().unwrap_or(Value::Null);
    let scope = atlas.pointer("/generated_from/scope").cloned().unwrap_or(Value::Null);
    out.push(format!("# Atlas — {}", serde_json::to_string(&scope).unwrap_or_default()));
    out.push(String::new());
    out.push(format!(
        "Source revision `{revision}`. {} projects · {} components · {} templates · {} routes · {} gates.",
        n(number(totals.get("projects"))),
        n(number(totals.get("components"))),
        n(number(totals.get("templates"))),
        n(number(totals.get("routes"))),
        n(number(totals.get("gates")))
    ));
    out.push(String::new());
    out.push("Derived from the tables beside it — join back on the ids in `atlas.json`. **reach** is the".into());
    out.push("cycle-safe closure of `render_graph` from a route's component: everything that screen can".into());
    out.push("render, transitively. An **area** is an NgModule, the frontend's own partitioning.".into());

    let empty = Vec::new();
    for p in atlas.get("projects").and_then(Value::as_array).unwrap_or(&empty) {
        out.push(String::new());
        let kind = match p.get("project_type") {
            Some(Value::String(s)) if !s.is_empty() => s.clone(),
            _ => "project".to_string(),
        };
        out.push(format!(
            "## {}  ({kind}, `{}/`)",
            p.get("name").and_then(Value::as_str).unwrap_or("None"),
            p.get("dir").and_then(Value::as_str).unwrap_or("None")
        ));
        out.push(String::new());
        out.push(format!(
            "{} components · {} NgModules · {} templates · {} gates · {} i18n keys · {} components in no module",
            n(number(p.get("components"))),
            n(number(p.get("ng_modules"))),
            n(number(p.get("templates"))),
            n(number(p.get("gates"))),
            n(number(p.get("i18n_keys"))),
            n(number(p.get("components_in_no_module")))
        ));
        out.push(String::new());
        let files: Vec<String> = p
            .get("files")
            .and_then(Value::as_object)
            .map(|f| {
                f.iter()
                    .map(|(ext, count)| {
                        let name = if ext == "None" || ext.is_empty() { "?" } else { ext.as_str() };
                        format!("{name}={}", n(count.as_i64().unwrap_or(0)))
                    })
                    .collect()
            })
            .unwrap_or_default();
        out.push(format!("files: {}", files.join(", ")));

        let bootstrap = p.get("bootstrap").and_then(Value::as_array).cloned().unwrap_or_default();
        if !bootstrap.is_empty() {
            out.push(String::new());
            let named: Vec<String> = bootstrap
                .iter()
                .map(|b| {
                    let name = b.get("name").and_then(Value::as_str).unwrap_or("?");
                    let file = b.get("file").and_then(Value::as_str).unwrap_or("outside the map");
                    format!("`{name}` ({file})")
                })
                .collect();
            out.push(format!("bootstrap: {}", named.join(", ")));
        }

        let routes = p.get("routes").and_then(Value::as_array).cloned().unwrap_or_default();
        if !routes.is_empty() {
            out.push(String::new());
            out.push("| route | component | reach — components / templates / gates / i18n | guards |".into());
            out.push("|---|---|---|---|".into());
            for r in &routes {
                let reach = match r.get("reach") {
                    Some(Value::Object(made)) => format!(
                        "{} / {} / {} / {}",
                        n(number(made.get("components"))),
                        n(number(made.get("templates"))),
                        n(number(made.get("gates"))),
                        n(number(made.get("i18n_keys")))
                    ),
                    _ if !matches!(r.get("lazy"), None | Some(Value::Null)) => {
                        "_lazy — target not statically knowable_".to_string()
                    }
                    _ => "_no component_".to_string(),
                };
                let component = match r.get("component") {
                    Some(Value::Null) | None => "—".to_string(),
                    Some(Value::String(name)) => format!(
                        "`{name}`{}",
                        if matches!(r.get("crosses_project"), Some(Value::Bool(true))) {
                            " ⟶ another project"
                        } else {
                            ""
                        }
                    ),
                    Some(other) => other.to_string(),
                };
                let guards = number(r.get("guards"));
                out.push(format!(
                    "| `{}` | {component} | {reach} | {} |",
                    cell(r.get("full_path").and_then(Value::as_str).unwrap_or("(no path)")),
                    if guards == 0 { String::new() } else { guards.to_string() }
                ));
            }
        }

        let areas = p.get("areas").and_then(Value::as_array).cloned().unwrap_or_default();
        if !areas.is_empty() {
            out.push(String::new());
            out.push("| NgModule | directory | components | templates | gates | i18n keys |".into());
            out.push("|---|---|---|---|---|---|".into());
            for a in &areas {
                out.push(format!(
                    "| `{}` | `{}` | {} | {} | {} | {} |",
                    a.get("name").and_then(Value::as_str).unwrap_or(""),
                    a.get("dir").and_then(Value::as_str).unwrap_or(""),
                    n(number(a.get("components"))),
                    n(number(a.get("templates"))),
                    n(number(a.get("gates"))),
                    n(number(a.get("i18n_keys")))
                ));
            }
        }
    }

    let boundaries = atlas.get("boundaries").and_then(Value::as_array).cloned().unwrap_or_default();
    if !boundaries.is_empty() {
        out.push(String::new());
        out.push("## Boundaries the map cannot cross".into());
        out.push(String::new());
        out.push("Stated, never omitted. A lazy route is listed here ONLY where its target stayed unresolved —".into());
        out.push("a runtime specifier with no file behind it, such as a federated remote. Where the checker did".into());
        out.push("resolve the import, the route is not a boundary and `routes.load_children_module` names the".into());
        out.push("declaration it loads. The source text is kept verbatim rather than parsed.".into());
        out.push(String::new());
        out.push("| project | route | kind | target |".into());
        out.push("|---|---|---|---|".into());
        for b in &boundaries {
            let project = match b.get("project") {
                Some(Value::String(s)) if !s.is_empty() => s.clone(),
                _ => "—".to_string(),
            };
            out.push(format!(
                "| {project} | `{}` | {} | `{}` |",
                cell(b.get("full_path").and_then(Value::as_str).unwrap_or("(no path)")),
                b.get("kind").and_then(Value::as_str).unwrap_or(""),
                cell(b.get("target").and_then(Value::as_str).unwrap_or(""))
            ));
        }
    }

    // WHAT THIS PROJECT HAS ALREADY WORKED OUT ABOUT ITSELF, if it said where.
    if let Some(Value::String(docs)) = atlas.get("project_docs") {
        out.push(String::new());
        out.push("## This workspace's own records".into());
        out.push(String::new());
        out.push(format!("`{}`", cell(docs)));
        out.push(String::new());
        out.push("Declared as `projectDocs`. Nothing in the extraction reads it — what a project has measured".into());
        out.push("about itself is knowledge about that project, so it lives there and this is the pointer.".into());
    }
    out.push(String::new());
    out.join("\n")
}

/// python's `json.dump(..., ensure_ascii=False, indent=1)`.
pub(super) fn pretty(value: &Value) -> String {
    let mut buffer = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b" ");
    let mut ser = serde_json::Serializer::with_formatter(&mut buffer, formatter);
    let _ = serde::Serialize::serialize(value, &mut ser);
    String::from_utf8_lossy(&buffer).into_owned()
}
