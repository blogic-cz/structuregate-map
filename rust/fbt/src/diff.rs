//! Comparing two trees: a fresh scan against a stored snapshot, or two stored
//! snapshots against each other.

use crate::entry::{Entry, Hash, Kind};
use anyhow::Result;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Added,
    Deleted,
    Modified,
    Renamed,
    /// Same path, but file became directory or link, or the other way round.
    TypeChanged,
}

impl ChangeKind {
    pub fn tag(self) -> char {
        match self {
            ChangeKind::Added => 'A',
            ChangeKind::Deleted => 'D',
            ChangeKind::Modified => 'M',
            ChangeKind::Renamed => 'R',
            ChangeKind::TypeChanged => 'T',
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Change {
    pub kind: ChangeKind,
    pub path: String,
    /// Where a renamed node came from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    pub node: &'static str,
}

fn node_name(k: Kind) -> &'static str {
    match k {
        Kind::Dir => "dir",
        Kind::File => "file",
        Kind::Link => "link",
    }
}

/// Minimal shape both sides of a comparison must offer.
#[derive(Clone)]
struct Side {
    path: String,
    kind: Kind,
    size: u64,
    file_id: Option<u64>,
    hash: Option<Hash>,
}

fn side_of(e: &Entry) -> Side {
    Side {
        path: e.path.clone(),
        kind: e.kind,
        size: e.size,
        file_id: e.file_id,
        hash: e.hash,
    }
}

/// Compare a stored snapshot with the entries of a fresh scan.
///
/// `include_dirs` off means a directory is not listed just because something
/// under it moved; the changed leaves already say that, and the directory rows
/// would triple the output.
pub fn diff_scan(old: &HashMap<String, Entry>, new: &[Entry], include_dirs: bool) -> Vec<Change> {
    let old_sides: HashMap<&str, Side> =
        old.iter().map(|(k, e)| (k.as_str(), side_of(e))).collect();
    let new_sides: HashMap<&str, Side> = new
        .iter()
        .map(|e| (e.path_key.as_str(), side_of(e)))
        .collect();
    compare(&old_sides, &new_sides, include_dirs)
}

/// Compare two snapshots already stored in the database.
pub fn diff_snapshots(
    conn: &Connection,
    old_id: i64,
    new_id: i64,
    include_dirs: bool,
) -> Result<Vec<Change>> {
    let load = |id: i64| -> Result<HashMap<String, Side>> {
        let mut stmt = conn.prepare(
            "SELECT path, path_key, kind, size, file_id, hash FROM entry WHERE snapshot_id = ?1",
        )?;
        let rows = stmt.query_map(params![id], |r| {
            let hash: Option<Vec<u8>> = r.get(5)?;
            Ok((
                r.get::<_, String>(1)?,
                Side {
                    path: r.get(0)?,
                    kind: Kind::from_i64(r.get(2)?),
                    size: r.get::<_, i64>(3)? as u64,
                    file_id: r.get::<_, Option<i64>>(4)?.map(|v| v as u64),
                    hash: hash.and_then(|b| <Hash>::try_from(b.as_slice()).ok()),
                },
            ))
        })?;
        let mut m = HashMap::new();
        for row in rows {
            let (k, v) = row?;
            m.insert(k, v);
        }
        Ok(m)
    };

    let old = load(old_id)?;
    let new = load(new_id)?;
    let old_ref: HashMap<&str, Side> = old.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    let new_ref: HashMap<&str, Side> = new.iter().map(|(k, v)| (k.as_str(), v.clone())).collect();
    Ok(compare(&old_ref, &new_ref, include_dirs))
}

fn compare(old: &HashMap<&str, Side>, new: &HashMap<&str, Side>, include_dirs: bool) -> Vec<Change> {
    let mut added: Vec<&Side> = Vec::new();
    let mut deleted: Vec<&Side> = Vec::new();
    let mut out: Vec<Change> = Vec::new();

    for (key, n) in new {
        if !include_dirs && n.kind == Kind::Dir {
            continue;
        }
        match old.get(key) {
            None => added.push(n),
            Some(o) => {
                if o.kind != n.kind {
                    out.push(Change {
                        kind: ChangeKind::TypeChanged,
                        path: n.path.clone(),
                        from: None,
                        node: node_name(n.kind),
                    });
                } else if o.hash != n.hash {
                    out.push(Change {
                        kind: ChangeKind::Modified,
                        path: n.path.clone(),
                        from: None,
                        node: node_name(n.kind),
                    });
                }
            }
        }
    }
    for (key, o) in old {
        if !include_dirs && o.kind == Kind::Dir {
            continue;
        }
        if !new.contains_key(key) {
            deleted.push(o);
        }
    }

    out.extend(pair_renames(&mut added, &mut deleted));
    for n in added {
        out.push(Change {
            kind: ChangeKind::Added,
            path: n.path.clone(),
            from: None,
            node: node_name(n.kind),
        });
    }
    for o in deleted {
        out.push(Change {
            kind: ChangeKind::Deleted,
            path: o.path.clone(),
            from: None,
            node: node_name(o.kind),
        });
    }

    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// Match an added node to a deleted one, and report the pair as a rename.
///
/// The file id is authoritative: NTFS keeps it across a move inside a volume.
/// Without one, an identical content hash and size is the next best evidence.
fn pair_renames<'a>(added: &mut Vec<&'a Side>, deleted: &mut Vec<&'a Side>) -> Vec<Change> {
    let mut out = Vec::new();
    if added.is_empty() || deleted.is_empty() {
        return out;
    }

    let mut by_file_id: HashMap<u64, usize> = HashMap::new();
    let mut by_hash: HashMap<(Hash, u64), usize> = HashMap::new();
    for (i, d) in deleted.iter().enumerate() {
        if let Some(id) = d.file_id {
            by_file_id.entry(id).or_insert(i);
        }
        if let Some(h) = d.hash {
            by_hash.entry((h, d.size)).or_insert(i);
        }
    }

    let mut used_add = vec![false; added.len()];
    let mut used_del = vec![false; deleted.len()];

    for (ai, a) in added.iter().enumerate() {
        let hit = a
            .file_id
            .and_then(|id| by_file_id.get(&id).copied())
            .or_else(|| a.hash.and_then(|h| by_hash.get(&(h, a.size)).copied()));
        if let Some(di) = hit {
            if used_del[di] || deleted[di].kind != a.kind {
                continue;
            }
            used_add[ai] = true;
            used_del[di] = true;
            out.push(Change {
                kind: ChangeKind::Renamed,
                path: a.path.clone(),
                from: Some(deleted[di].path.clone()),
                node: node_name(a.kind),
            });
        }
    }

    let mut ai = 0;
    added.retain(|_| {
        let keep = !used_add[ai];
        ai += 1;
        keep
    });
    let mut di = 0;
    deleted.retain(|_| {
        let keep = !used_del[di];
        di += 1;
        keep
    });
    out
}
