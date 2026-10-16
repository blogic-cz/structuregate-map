//! WHICH C# ACTION AN ANGULAR HTTP CALL REACHES - one pass over the finished rows of both halves.
//!
//! `http_endpoints` is every attribute-routed action (`endpoints.rs`), `http_calls` every `@angular/common/http`
//! call, a wrapper's callers standing for the wrapper's own call (`calls.rs`), and `http_links` which call reaches
//! which endpoint (`matching.rs`). Rewritten whole by each run that does not replay it (`inputs.rs`): it reads nothing
//! but the rows the halves stored.
//! A database with no TypeScript still gets its C# endpoints; `http_calls` and `http_links` are then empty.

mod calls;
mod endpoints;
pub mod inputs;
mod matching;

use super::dups::{has_columns, text};
use rusqlite::{params, Connection};

/// What a run wrote: endpoints, call sites, calls linked to exactly one endpoint, calls no endpoint matched, calls
/// several endpoints matched.
#[derive(Default, Debug, PartialEq)]
pub struct Counts {
    pub endpoints: usize,
    pub calls: usize,
    pub linked: usize,
    pub unmatched: usize,
    pub ambiguous: usize,
}

/// The files of one language: id -> path.
pub(crate) fn files_of(conn: &Connection, lang: &str) -> rusqlite::Result<std::collections::HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT id, path FROM files WHERE lang = ?1")?;
    let rows = stmt.query_map([lang], |r| Ok((text(r, 0)?, text(r, 1)?)))?;
    rows.collect()
}

/// The three tables rewritten whole. Their sizes.
pub fn write(conn: &Connection) -> rusqlite::Result<Counts> {
    // A database no half has written to has no `files` and nothing to join.
    if !has_columns(conn, "files", &["id", "path", "lang"]) {
        return Ok(Counts::default());
    }
    // Rewritten whole, node tables and all - these hold none today, and one a later column made would outlive them.
    for table in ["http_endpoints", "http_calls", "http_links"] {
        super::cells::drop_children(conn, table).map_err(|e| rusqlite::Error::InvalidParameterName(format!("{e:#}")))?;
    }
    conn.execute_batch(
        "DROP TABLE IF EXISTS http_endpoints; DROP TABLE IF EXISTS http_calls; DROP TABLE IF EXISTS http_links;
         CREATE TABLE http_endpoints (id, file, cls, method, line, verb, template, route);
         CREATE TABLE http_calls (id, file, call, member, line, verb, url, via);
         CREATE TABLE http_links (call, endpoint, how);",
    )?;
    let (csharp, typescript) = (files_of(conn, "csharp")?, files_of(conn, "typescript")?);
    // THE ENDPOINTS ARE A FACT ABOUT THE C# TREE ALONE (#59): a C#-only consumer asks which action serves a URL
    // too. Only the calls and the links need the TypeScript half.
    let endpoints = endpoints::load(conn, &csharp)?;
    let sites = if typescript.is_empty() { Vec::new() } else { calls::load(conn, &typescript)? };
    let linked = matching::link(&endpoints, &sites);
    let mut counts = Counts { endpoints: endpoints.len(), calls: sites.len(), ..Counts::default() };
    {
        let mut add = conn.prepare("INSERT INTO http_endpoints VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)")?;
        for (n, e) in endpoints.iter().enumerate() {
            add.execute(params![format!("he:{}", n + 1), e.file, e.cls, e.method, e.line, e.verb, e.template, e.route])?;
        }
        let mut add = conn.prepare("INSERT INTO http_calls VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)")?;
        for (n, s) in sites.iter().enumerate() {
            add.execute(params![format!("hc:{}", n + 1), s.file, s.call, s.member, s.line, s.verb, s.url, s.via])?;
        }
        let mut add = conn.prepare("INSERT INTO http_links VALUES (?1, ?2, ?3)")?;
        for (n, reached) in linked.iter().enumerate() {
            match reached.len() {
                0 => counts.unmatched += 1,
                1 => counts.linked += 1,
                _ => counts.ambiguous += 1,
            }
            let how = if reached.len() == 1 { "exact" } else { "ambiguous" };
            for e in reached {
                add.execute(params![format!("hc:{}", n + 1), format!("he:{}", e + 1), how])?;
            }
        }
    }
    conn.execute_batch("CREATE INDEX ix_http_links_call ON http_links(call); CREATE INDEX ix_http_links_endpoint ON http_links(endpoint);")?;
    Ok(counts)
}
