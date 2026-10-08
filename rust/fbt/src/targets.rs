//! Build targets: which target owns a path, and what has to rebuild with it.
//!
//! A change list on its own does not say what to do. A target names a unit the
//! build can rebuild, its input patterns say which paths belong to it, and the
//! dependency edges carry a change outward: if `core` is dirty, everything that
//! depends on `core` is dirty too.

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TargetSpec {
    pub name: String,
    /// Glob patterns relative to the scan root, for example `crates/core/**`.
    #[serde(default)]
    pub inputs: Vec<String>,
    /// Names of targets this one is built from.
    #[serde(default)]
    pub deps: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TargetFile {
    pub targets: Vec<TargetSpec>,
}

/// Replace the whole target graph with the contents of a JSON file.
pub fn import(conn: &mut Connection, path: &Path) -> Result<usize> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read {}", path.display()))?;
    let file: TargetFile = serde_json::from_str(&text)
        .with_context(|| format!("{} is not a valid target file", path.display()))?;

    let tx = conn.transaction()?;
    tx.execute("DELETE FROM target", [])?;

    for t in &file.targets {
        tx.execute("INSERT INTO target(name) VALUES (?1)", params![t.name])?;
        let id = tx.last_insert_rowid();
        for p in &t.inputs {
            tx.execute(
                "INSERT OR IGNORE INTO target_input(target_id, pattern) VALUES (?1, ?2)",
                params![id, p],
            )?;
        }
    }
    // Dependencies second, so a target may be named before it is declared.
    for t in &file.targets {
        for d in &t.deps {
            tx.execute(
                "INSERT OR IGNORE INTO target_dep(target_id, depends_on)
                 SELECT a.id, b.id FROM target a, target b WHERE a.name = ?1 AND b.name = ?2",
                params![t.name, d],
            )?;
        }
    }
    tx.commit()?;
    Ok(file.targets.len())
}

pub fn list(conn: &Connection) -> Result<Vec<TargetSpec>> {
    let mut stmt = conn.prepare("SELECT id, name FROM target ORDER BY name")?;
    let rows: Vec<(i64, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;

    let mut out = Vec::new();
    for (id, name) in rows {
        let mut in_stmt =
            conn.prepare("SELECT pattern FROM target_input WHERE target_id = ?1 ORDER BY pattern")?;
        let inputs: Vec<String> = in_stmt
            .query_map(params![id], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        let mut dep_stmt = conn.prepare(
            "SELECT t.name FROM target_dep d JOIN target t ON t.id = d.depends_on
             WHERE d.target_id = ?1 ORDER BY t.name",
        )?;
        let deps: Vec<String> = dep_stmt
            .query_map(params![id], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        out.push(TargetSpec { name, inputs, deps });
    }
    Ok(out)
}

/// Targets whose own inputs match at least one changed path.
fn seeds(conn: &Connection, changed: &[String]) -> Result<Vec<i64>> {
    let mut stmt = conn.prepare("SELECT target_id, pattern FROM target_input")?;
    let rows: Vec<(i64, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;

    let mut by_target: HashMap<i64, GlobSetBuilder> = HashMap::new();
    for (id, pattern) in rows {
        let glob = Glob::new(&pattern)
            .with_context(|| format!("target pattern {pattern} is not a valid glob"))?;
        by_target.entry(id).or_insert_with(GlobSetBuilder::new).add(glob);
    }

    let built: Vec<(i64, GlobSet)> = by_target
        .into_iter()
        .filter_map(|(id, b)| b.build().ok().map(|s| (id, s)))
        .collect();

    let mut hit = Vec::new();
    for (id, set) in built {
        if changed.iter().any(|p| set.is_match(p)) {
            hit.push(id);
        }
    }
    Ok(hit)
}

/// Every target that must rebuild: the ones owning a changed path, plus
/// everything that depends on those, transitively.
pub fn dirty(conn: &Connection, changed: &[String]) -> Result<Vec<String>> {
    let seed_ids = seeds(conn, changed)?;
    if seed_ids.is_empty() {
        return Ok(Vec::new());
    }

    conn.execute_batch("CREATE TEMP TABLE IF NOT EXISTS seed(id INTEGER PRIMARY KEY); DELETE FROM seed;")?;
    {
        let mut ins = conn.prepare("INSERT OR IGNORE INTO seed(id) VALUES (?1)")?;
        for id in &seed_ids {
            ins.execute(params![id])?;
        }
    }

    let mut stmt = conn.prepare(
        "WITH RECURSIVE dirty(id) AS (
             SELECT id FROM seed
             UNION
             SELECT d.target_id FROM target_dep d JOIN dirty ON d.depends_on = dirty.id
         )
         SELECT t.name FROM target t JOIN dirty ON t.id = dirty.id ORDER BY t.name",
    )?;
    let names: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    Ok(names)
}

/// Changed paths that no target claims. Worth printing: it usually means an
/// input pattern is missing, and the build would silently skip the change.
pub fn unclaimed(conn: &Connection, changed: &[String]) -> Result<Vec<String>> {
    let mut stmt = conn.prepare("SELECT pattern FROM target_input")?;
    let patterns: Vec<String> = stmt
        .query_map([], |r| r.get(0))?
        .collect::<std::result::Result<_, _>>()?;
    let mut b = GlobSetBuilder::new();
    for p in &patterns {
        if let Ok(g) = Glob::new(p) {
            b.add(g);
        }
    }
    let set = b.build()?;
    Ok(changed.iter().filter(|p| !set.is_match(p)).cloned().collect())
}
