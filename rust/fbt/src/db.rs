//! SQLite storage: schema, snapshot rows, bulk entry writes.

use crate::entry::{Entry, Hash, Kind};
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::path::Path;

pub const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS snapshot (
  id          INTEGER PRIMARY KEY,
  root        TEXT    NOT NULL,
  root_key    TEXT    NOT NULL,
  created_ns  INTEGER NOT NULL,
  usn         INTEGER,
  journal_id  INTEGER,
  volume      TEXT,
  usn_chain   INTEGER NOT NULL DEFAULT 0,
  entry_count INTEGER NOT NULL DEFAULT 0,
  root_hash   BLOB
);

CREATE INDEX IF NOT EXISTS ix_snapshot_root ON snapshot(root_key, id DESC);

CREATE TABLE IF NOT EXISTS entry (
  snapshot_id INTEGER NOT NULL,
  path        TEXT    NOT NULL,
  path_key    TEXT    NOT NULL,
  parent_key  TEXT,
  kind        INTEGER NOT NULL,
  size        INTEGER NOT NULL,
  mtime_ns    INTEGER NOT NULL,
  file_id     INTEGER,
  hash        BLOB,
  PRIMARY KEY (snapshot_id, path_key)
) WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS ix_entry_parent ON entry(snapshot_id, parent_key);
CREATE INDEX IF NOT EXISTS ix_entry_fileid ON entry(snapshot_id, file_id);
CREATE INDEX IF NOT EXISTS ix_entry_hash   ON entry(snapshot_id, hash);

CREATE TABLE IF NOT EXISTS target (
  id   INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS target_input (
  target_id INTEGER NOT NULL REFERENCES target(id) ON DELETE CASCADE,
  pattern   TEXT    NOT NULL,
  PRIMARY KEY (target_id, pattern)
);

CREATE TABLE IF NOT EXISTS target_dep (
  target_id  INTEGER NOT NULL REFERENCES target(id) ON DELETE CASCADE,
  depends_on INTEGER NOT NULL REFERENCES target(id) ON DELETE CASCADE,
  PRIMARY KEY (target_id, depends_on)
);

CREATE INDEX IF NOT EXISTS ix_dep_reverse ON target_dep(depends_on);
"#;

/// A stored scan of one root.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub id: i64,
    pub root: String,
    pub created_ns: i64,
    pub usn: Option<i64>,
    pub journal_id: Option<i64>,
    pub volume: Option<String>,
    /// How many journal runs in a row this snapshot rests on. A full walk resets
    /// it to zero. See `MAX_USN_CHAIN` in `pipeline`.
    pub usn_chain: i64,
    pub entry_count: i64,
    pub root_hash: Option<Hash>,
}

pub fn open(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)
        .with_context(|| format!("cannot open database {}", path.display()))?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "cache_size", -64_000i64)?;
    conn.execute_batch(SCHEMA)?;
    conn.execute(
        "INSERT INTO meta(key, value) VALUES('schema_version', ?1)
         ON CONFLICT(key) DO NOTHING",
        params![SCHEMA_VERSION.to_string()],
    )?;
    Ok(conn)
}

fn to_hash(v: Option<Vec<u8>>) -> Option<Hash> {
    v.and_then(|b| <Hash>::try_from(b.as_slice()).ok())
}

fn read_snapshot(r: &rusqlite::Row<'_>) -> rusqlite::Result<Snapshot> {
    Ok(Snapshot {
        id: r.get(0)?,
        root: r.get(1)?,
        created_ns: r.get(2)?,
        usn: r.get(3)?,
        journal_id: r.get(4)?,
        volume: r.get(5)?,
        usn_chain: r.get(6)?,
        entry_count: r.get(7)?,
        root_hash: to_hash(r.get(8)?),
    })
}

const SNAPSHOT_COLS: &str =
    "id, root, created_ns, usn, journal_id, volume, usn_chain, entry_count, root_hash";

pub fn insert_snapshot(
    conn: &Connection,
    root: &str,
    created_ns: i64,
    usn: Option<i64>,
    journal_id: Option<i64>,
    volume: Option<&str>,
    usn_chain: i64,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO snapshot(root, root_key, created_ns, usn, journal_id, volume, usn_chain)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![root, root.to_lowercase(), created_ns, usn, journal_id, volume, usn_chain],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Move an unchanged snapshot forward: the time it answers for and the journal position it was taken at.
pub fn advance_snapshot(
    conn: &Connection,
    snapshot_id: i64,
    created_ns: i64,
    usn: Option<i64>,
    journal_id: Option<i64>,
    usn_chain: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE snapshot SET created_ns = ?2, usn = ?3, journal_id = ?4, usn_chain = ?5 WHERE id = ?1",
        params![snapshot_id, created_ns, usn, journal_id, usn_chain],
    )?;
    Ok(())
}

pub fn finish_snapshot(
    conn: &Connection,
    snapshot_id: i64,
    entry_count: i64,
    root_hash: Option<Hash>,
) -> Result<()> {
    conn.execute(
        "UPDATE snapshot SET entry_count = ?2, root_hash = ?3 WHERE id = ?1",
        params![snapshot_id, entry_count, root_hash.map(|h| h.to_vec())],
    )?;
    Ok(())
}

pub fn latest_snapshot(conn: &Connection, root: &str) -> Result<Option<Snapshot>> {
    let sql = format!(
        "SELECT {SNAPSHOT_COLS} FROM snapshot WHERE root_key = ?1 ORDER BY id DESC LIMIT 1"
    );
    Ok(conn
        .query_row(&sql, params![root.to_lowercase()], read_snapshot)
        .optional()?)
}

pub fn snapshot_by_id(conn: &Connection, id: i64) -> Result<Option<Snapshot>> {
    let sql = format!("SELECT {SNAPSHOT_COLS} FROM snapshot WHERE id = ?1");
    Ok(conn.query_row(&sql, params![id], read_snapshot).optional()?)
}

pub fn list_snapshots(conn: &Connection, limit: i64) -> Result<Vec<Snapshot>> {
    let sql = format!("SELECT {SNAPSHOT_COLS} FROM snapshot ORDER BY id DESC LIMIT ?1");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![limit], read_snapshot)?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

/// Write every entry of one scan in a single transaction.
pub fn insert_entries(conn: &mut Connection, snapshot_id: i64, entries: &[Entry]) -> Result<()> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO entry
               (snapshot_id, path, path_key, parent_key, kind, size, mtime_ns, file_id, hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        )?;
        for e in entries {
            stmt.execute(params![
                snapshot_id,
                e.path,
                e.path_key,
                e.parent_key,
                e.kind as i64,
                e.size as i64,
                e.mtime_ns,
                e.file_id.map(|v| v as i64),
                e.hash.map(|h| h.to_vec()),
            ])?;
        }
    }
    tx.commit()?;
    Ok(())
}

/// Load a whole snapshot keyed by `path_key`. This is the stat cache that lets a
/// rescan skip reading the content of unchanged files.
pub fn load_entries(conn: &Connection, snapshot_id: i64) -> Result<HashMap<String, Entry>> {
    let mut stmt = conn.prepare(
        "SELECT path, path_key, parent_key, kind, size, mtime_ns, file_id, hash
         FROM entry WHERE snapshot_id = ?1",
    )?;
    let rows = stmt.query_map(params![snapshot_id], |r| {
        Ok(Entry {
            path: r.get(0)?,
            path_key: r.get(1)?,
            parent_key: r.get(2)?,
            kind: Kind::from_i64(r.get(3)?),
            size: r.get::<_, i64>(4)? as u64,
            mtime_ns: r.get(5)?,
            file_id: r.get::<_, Option<i64>>(6)?.map(|v| v as u64),
            hash: to_hash(r.get(7)?),
        })
    })?;
    let mut map = HashMap::new();
    for row in rows {
        let e = row?;
        map.insert(e.path_key.clone(), e);
    }
    Ok(map)
}

/// Keep only the newest `keep` snapshots per root; delete the rest with their entries.
pub fn prune(conn: &Connection, keep: i64) -> Result<usize> {
    let n = conn.execute(
        "DELETE FROM snapshot WHERE id IN (
           SELECT id FROM (
             SELECT id, ROW_NUMBER() OVER (PARTITION BY root_key ORDER BY id DESC) AS rn
             FROM snapshot
           ) WHERE rn > ?1)",
        params![keep],
    )?;
    conn.execute(
        "DELETE FROM entry WHERE snapshot_id NOT IN (SELECT id FROM snapshot)",
        [],
    )?;
    Ok(n)
}
