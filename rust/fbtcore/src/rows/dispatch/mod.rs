//! WHICH HANDLER A MEDIATOR CALL RUNS (#63) - `dispatch`, derived from the C# rows after every half, rewritten whole.
//!
//! `sender.Send(new ArchivedOrdersQuery(p))` binds to the generic `ISender.Send<T>`, and nothing says the code that runs
//! is the class implementing `IRequestHandler<ArchivedOrdersQuery, ..>`. The link is a JOIN the rows already hold: the
//! call's first argument's STATIC type (`arguments.type`) against the closed bases of every class (`classes.bases`).
//!
//! - a call of a method named `Send` reaches every class with a base `IRequestHandler<T>` or `IRequestHandler<T, R>`,
//!   `T` the argument's type exactly (`kind = 'request'`; MediatR has one handler - several are listed as found);
//! - `Publish` reaches every `INotificationHandler<T>` (`kind = 'notification'`, several by design).
//!
//! BY THE STATIC TYPE, never a subtype: a handler of a base message runs for a derived one only by the mediator's own
//! variance rules, which the rows do not state. An argument typed by its interface (`IRequest<R> q`) has no exact handler
//! and is no row. Type texts compare as Roslyn writes them; a message named by a keyword (`string`) matches nothing.

use super::dups::{has_columns, text};
use rusqlite::{params, Connection};
use std::collections::HashMap;

/// A base's interface name without namespace and its type arguments, top-level commas only: `A.IRequestHandler<X<Y, Z>, R>`
/// is (`IRequestHandler`, [`X<Y, Z>`, `R`]).
fn generic(base: &str) -> Option<(&str, Vec<String>)> {
    let open = base.find('<')?;
    let inner = base[open + 1..].strip_suffix('>')?;
    let name = base[..open].rsplit('.').next()?;
    let (mut args, mut depth, mut piece) = (Vec::new(), 0usize, String::new());
    for c in inner.chars() {
        match c {
            '<' | '[' | '(' => depth += 1,
            '>' | ']' | ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                args.push(piece.trim().to_string());
                piece.clear();
                continue;
            }
            _ => {}
        }
        piece.push(c);
    }
    args.push(piece.trim().to_string());
    Some((name, args))
}

/// The method a call's symbol names: `MediatR.ISender.Send<System.String[]>` is `Send`.
fn method(symbol: &str) -> &str {
    let bare = symbol.split('<').next().unwrap_or(symbol);
    bare.rsplit('.').next().unwrap_or(bare)
}

struct Handler {
    symbol: String,
    file: String,
    line: String,
}

/// The table rewritten whole. Its size.
pub fn write(conn: &Connection) -> rusqlite::Result<usize> {
    conn.execute_batch("DROP TABLE IF EXISTS dispatch; CREATE TABLE dispatch (call, file, line, func, kind, message, handler, handler_file, handler_line);")?;
    let ready = has_columns(conn, "calls", &["id", "file", "line", "func", "symbol"])
        && has_columns(conn, "arguments", &["call", "position", "type"])
        && has_columns(conn, "classes", &["file", "line", "symbol", "bases"]);
    if !ready {
        return Ok(0);
    }
    // (interface, message type) -> its handlers.
    let mut handlers: HashMap<(&'static str, String), Vec<Handler>> = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT c.symbol, c.file, c.line, c.bases FROM classes c JOIN files f ON f.id = c.file WHERE f.lang = 'csharp'")?;
        let rows = stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?)))?;
        for row in rows {
            let (symbol, file, line, bases) = row?;
            for base in serde_json::from_str::<Vec<String>>(&bases).unwrap_or_default() {
                let Some((name, args)) = generic(&base) else { continue };
                let kind = match (name, args.len()) {
                    ("IRequestHandler", 1 | 2) => "request",
                    ("INotificationHandler", 1) => "notification",
                    _ => continue,
                };
                handlers.entry((kind, args[0].clone())).or_default().push(Handler { symbol: symbol.clone(), file: file.clone(), line: line.clone() });
            }
        }
    }
    if handlers.is_empty() {
        return Ok(0);
    }
    let mut stmt = conn.prepare(
        "SELECT c.id, c.file, c.line, c.func, c.symbol, a.type FROM calls c JOIN arguments a ON a.call = c.id AND a.position = 0 \
         JOIN files f ON f.id = c.file WHERE f.lang = 'csharp' ORDER BY c.file, c.line, c.id",
    )?;
    let rows: Vec<(String, String, String, String, String, String)> =
        stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, text(r, 2)?, text(r, 3)?, text(r, 4)?, text(r, 5)?)))?.collect::<rusqlite::Result<_>>()?;
    let mut add = conn.prepare("INSERT INTO dispatch VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)")?;
    let mut written = 0;
    for (call, file, line, func, symbol, message) in rows {
        let kind = match method(&symbol) {
            "Send" => "request",
            "Publish" => "notification",
            _ => continue,
        };
        for h in handlers.get(&(kind, message.clone())).into_iter().flatten() {
            add.execute(params![call, file, line, func, kind, message, format!("{}.Handle", h.symbol), h.file, h.line])?;
            written += 1;
        }
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_base_splits_at_its_top_level_commas() {
        let (name, args) = generic("MediatR.IRequestHandler<Demo.Q<int, string>, System.String[]>").unwrap();
        assert_eq!(name, "IRequestHandler");
        assert_eq!(args, vec!["Demo.Q<int, string>".to_string(), "System.String[]".to_string()]);
        assert!(generic("Demo.Plain").is_none());
        assert_eq!(method("MediatR.ISender.Send<System.String[]>"), "Send");
        assert_eq!(method("Demo.Bus.Publish"), "Publish");
    }

    #[test]
    fn a_send_reaches_the_handler_closed_over_its_argument_type_and_nothing_else() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE files (id, lang); INSERT INTO files VALUES ('f:1', 'csharp');
             CREATE TABLE classes (file, line, symbol, bases);
             INSERT INTO classes VALUES ('f:1', 4, 'Demo.QHandler', '[\"MediatR.IRequestHandler<Demo.Q, System.String[]>\"]');
             INSERT INTO classes VALUES ('f:1', 9, 'Demo.OtherHandler', '[\"MediatR.IRequestHandler<Demo.Other, int>\"]');
             CREATE TABLE calls (id, file, line, func, symbol);
             INSERT INTO calls VALUES ('call:1', 'f:1', 12, 'Get', 'MediatR.ISender.Send<System.String[]>');
             INSERT INTO calls VALUES ('call:2', 'f:1', 13, 'Get', 'Demo.Log.Write');
             CREATE TABLE arguments (call, position, type);
             INSERT INTO arguments VALUES ('call:1', 0, 'Demo.Q'); INSERT INTO arguments VALUES ('call:2', 0, 'Demo.Q');",
        )
        .unwrap();
        assert_eq!(write(&conn).unwrap(), 1);
        let handler: String = conn.query_row("SELECT handler FROM dispatch WHERE call = 'call:1'", [], |r| r.get(0)).unwrap();
        assert_eq!(handler, "Demo.QHandler.Handle");
    }
}
