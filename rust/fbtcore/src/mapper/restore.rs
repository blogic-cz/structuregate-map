//! WHETHER THE C# PROJECTS WERE RESTORED, said as a WARNING in the map summary: Roslyn binds a project whatever its
//! restore left on disk, so without these notes a tree missing its packages looked fully mapped.

use super::protocol::Collector;
use std::path::Path;

/// What a note about projects that were never restored starts with - the map's summary says it as a WARNING.
pub const UNRESTORED: &str = "C# project(s) never restored";

/// What a note about restored projects whose packages have since left the package folder starts with - a WARNING too.
pub const PACKAGES_GONE: &str = "C# project(s) missing restored packages";

/// A TREE WHOSE PROJECTS WERE NEVER RESTORED IS SAID TO BE ONE. Roslyn still binds it - without the NuGet
/// packages, the referenced assets or the generators - so every count looked right while hundreds of files carried
/// compile errors and every resolved column was empty. An SDK project with no `obj/project.assets.json` is one;
/// the list is kept in `_meta` as `unrestored:csharp`, so a reader of the database can tell too.
pub(crate) fn unrestored(into: &mut Collector, db: &str, root: &str, projects: &[String]) {
    let missing: Vec<&String> = projects.iter()
        .filter(|csproj| crate::csproj::sdk_style(csproj))
        .filter(|csproj| crate::csproj::parent(csproj).is_none_or(|folder| !Path::new(&folder).join("obj").join("project.assets.json").is_file()))
        .collect();
    let named: Vec<String> = missing.iter()
        .map(|p| Path::new(p).strip_prefix(root).map_or_else(|_| p.to_string(), |r| r.to_string_lossy().replace('\\', "/")))
        .collect();
    // RESTORED, AND ITS PACKAGES GONE SINCE: what a cleared NuGet folder leaves - each with how many it misses.
    let gone: Vec<(String, usize)> = projects.iter()
        .filter(|csproj| crate::csproj::sdk_style(csproj) && !missing.contains(csproj))
        .filter_map(|csproj| {
            let lost = crate::csproj::parent(csproj).map(|folder| crate::csproj::missing_packages(&folder)).unwrap_or_default();
            let named = Path::new(csproj).strip_prefix(root).map_or_else(|_| csproj.to_string(), |r| r.to_string_lossy().replace('\\', "/"));
            (!lost.is_empty()).then_some((named, lost.len()))
        })
        .collect();
    let Ok(conn) = rusqlite::Connection::open(db) else { return };
    // WRITTEN ONLY WHEN IT CHANGES: a run that changed nothing writes nothing.
    let listed = serde_json::Value::from(named.clone()).to_string();
    if crate::rows::meta::get(&conn, "unrestored:csharp").as_deref() != Some(listed.as_str()) {
        let _ = crate::rows::meta::put(&conn, "unrestored:csharp", &listed);
    }
    if named.is_empty() && gone.is_empty() {
        return;
    }
    let broken: i64 = conn.query_row("SELECT count(*) FROM files WHERE lang = 'csharp' AND errors > 0", [], |r| r.get(0)).unwrap_or(0);
    if !gone.is_empty() {
        let shown: Vec<String> = gone.iter().take(3).map(|(p, n)| format!("{p} ({n})")).collect();
        into.notes.push(format!(
            "{} {PACKAGES_GONE} (restored, but packages its obj/project.assets.json lists are in no package folder: {}{}) - Roslyn \
             bound them without those packages, and {broken} C# file(s) have compile errors; run `dotnet restore`",
            gone.len(), shown.join(", "), if gone.len() > 3 { ", …" } else { "" }
        ));
    }
    if named.is_empty() {
        return;
    }
    let shown: Vec<&str> = named.iter().take(3).map(String::as_str).collect();
    into.notes.push(format!(
        "{} {UNRESTORED} (no obj/project.assets.json: {}{}) - Roslyn bound them without their packages, project assets or \
         generators, and {broken} C# file(s) have compile errors; run `dotnet restore`",
        named.len(), shown.join(", "), if named.len() > 3 { ", …" } else { "" }
    ));
}
