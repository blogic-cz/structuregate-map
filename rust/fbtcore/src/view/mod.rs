//! `--map-view <db>`: THE MAP AS A PAGE - one self-contained HTML file a person opens in a browser, drawn from the
//! deep map's database. Four views over one graph (`model.rs`): the STRUCTURE (folders as nodes, drilled into by a
//! double click, rendered by WebGL), a NEIGHBOURHOOD (what calls a file or function and what it calls, N hops), the
//! DEPENDENCY MATRIX (folders by folders, a cycle shown as a cell above the diagonal) and a TREEMAP (area by lines,
//! colour by language, by what nothing reaches, by errors).
//!
//! NOTHING IS FETCHED: the page carries its data and every script, so it opens offline in any tree the gate is
//! shipped into. The libraries are vendored under `src/MapView/vendor/` with their licences (sigma.js, graphology,
//! graphology-library: MIT) and compiled INTO this exe, like every other script it runs.

mod model;

use crate::cli::Out;
use std::path::Path;

const PAGE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../src/MapView/MapView.html"));
const STYLE: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../src/MapView/MapView.css"));
const SCRIPTS: [&str; 7] = [
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../src/MapView/vendor/graphology.umd.min.js")),
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../src/MapView/vendor/graphology-library.min.js")),
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../src/MapView/vendor/sigma.min.js")),
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../src/MapView/MapView.Model.js")),
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../src/MapView/MapView.Graph.js")),
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../src/MapView/MapView.Panels.js")),
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../src/MapView/MapView.App.js")),
];

/// Write the page for `db` to `out` (beside the database, as `<name>.html`, when not given). 0, or 2 and why not.
/// `graph` is the file-level map; without one, a `buildmap.json` beside the database is read when it is there.
pub fn run(db: &str, out: Option<&str>, graph: Option<&str>, out_lines: &mut Out) -> i64 {
    let beside = Path::new(db).with_file_name("buildmap.json");
    let graph = graph.map(String::from).or_else(|| beside.is_file().then(|| beside.to_string_lossy().into_owned()));
    let page = match build(db, graph.as_deref()) {
        Ok(page) => page,
        Err(why) => {
            out_lines.error(format!("structuregate: the map view of {db} could not be built ({why:#})"));
            return 2;
        }
    };
    let target = out.map(String::from).unwrap_or_else(|| Path::new(db).with_extension("html").to_string_lossy().into_owned());
    if let Err(why) = std::fs::write(&target, page.as_bytes()) {
        out_lines.error(format!("structuregate: the map view could not be written to {target} ({why})"));
        return 2;
    }
    let read = graph.map(|g| format!(", file edges from {g}")).unwrap_or_default();
    out_lines.line(format!("structuregate map view: {target} ({} KB{read}) - open it in a browser", page.len() / 1024));
    0
}

fn build(db: &str, graph: Option<&str>) -> anyhow::Result<String> {
    anyhow::ensure!(Path::new(db).is_file(), "no such database");
    let conn = Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let graph = match graph {
        Some(path) => Some(serde_json::from_str::<serde_json::Value>(&std::fs::read_to_string(path)?)
            .map_err(|e| anyhow::anyhow!("{path} is not a file-level map ({e})"))?),
        None => None,
    };
    let mut data = model::read(&conn, graph.as_ref())?;
    data["db"] = serde_json::Value::from(db);
    // `</` CANNOT APPEAR INSIDE A SCRIPT ELEMENT: a path or a name holding `</script>` would end it early.
    let json = data.to_string().replace("</", "<\\/");
    let scripts: String = SCRIPTS.iter().map(|s| format!("<script>\n{}\n</script>\n", s.replace("</script", "<\\/script"))).collect();
    Ok(PAGE.replacen("/*STYLE*/", STYLE, 1).replacen("<!--DATA-->", &json, 1).replacen("<!--SCRIPTS-->", &scripts, 1))
}

use rusqlite::Connection;
