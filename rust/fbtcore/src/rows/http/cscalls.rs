//! THE `System.Net.Http` CALLS OF A C# TREE (#143), each with the url it asks for as a pattern - the C# side of `http_calls`.
//!
//! A PRIMITIVE is a call Roslyn bound to `HttpClient` or to `HttpClientJsonExtensions` whose name says its verb (`Get*`,
//! `Post*`, `Put*`, `Patch*`, `Delete*`); its url is the argument bound to `requestUri`. A `Send`/`SendAsync` takes its verb
//! and url from the `new HttpRequestMessage(HttpMethod.X, url)` it is handed - written in its argument, or the one the
//! local it names was declared with. A url is FOLDED as the half folded it: a constant is itself, an interpolation its
//! text with each hole `{}` (`arguments.template`, the query cut) - alternatives that name several paths (a ternary) a row for
//! each - and anything else an unknown `*`. A url that IS a
//! parameter of the method it is written in makes that method a WRAPPER: it writes no call itself, and every call OF it
//! does, with the url its caller binds to that parameter and `via` the wrapper's `functions` id - so a wrapper of a
//! wrapper is followed too. A wrapper whose url is a `string.Format` of its parameter FORMATS the caller's url: each `{n}`
//! in it is a hole `{}`. Text BEFORE the parameter (`Prefix + uri`, past any hole the run time fills first) is a PREFIX each
//! caller's url is joined to - and a wrapper with a prefix that nothing in the tree calls (an interface's
//! implementation) is its own row, its url as written (the parameter a hole `{}`), as it was before a caller could be found. A wrapper whose primitives ask with different verbs asks with `ANY`. A call is OF a wrapper by its bound symbol, which is the wrapper's qualname; a qualname two
//! methods share (an overload) is no wrapper, since the symbol cannot say which of them runs.
//!
//! A WRAPPER IS ALSO EACH INTERFACE OR BASE METHOD IT IMPLEMENTS (`classes.bases`, every base above it): a call through
//! `IClient.Post` - a DI field, a `Lazy<IClient>` - is bound to the interface, and is a call of the wrapper; its url is the
//! argument of the wrapper's parameter name, else the one at that parameter's place (the interface may name it otherwise).
//! A url alternative of holes alone (a ternary's arm only the run time knows) is left out while another one says a path.

use super::calls::Site;
use crate::rows::dups::{scalar_sql, text, Column};
use rusqlite::types::Value as SqlValue;
use rusqlite::Connection;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

const CLASSES: [&str; 2] = ["System.Net.Http.HttpClient.", "System.Net.Http.Json.HttpClientJsonExtensions."];
const VERBS: [(&str, &str); 5] = [("Get", "GET"), ("Post", "POST"), ("Put", "PUT"), ("Patch", "PATCH"), ("Delete", "DELETE")];
/// A request message's construction: its constructor, or the type alone where Roslyn bound no overload.
const MESSAGE: [&str; 2] = ["System.Net.Http.HttpRequestMessage.HttpRequestMessage", "System.Net.Http.HttpRequestMessage"];
const METHOD: &str = "System.Net.Http.HttpMethod.";
const URI: &str = "System.Uri.Uri";

struct Call {
    id: String,
    file: String,
    line: i64,
    end: i64,
    symbol: String,
    /// The innermost `functions` row holding the call: its id, qualname and first line.
    member: String,
    qualname: String,
    start: i64,
}

struct Arg {
    name: String,
    position: i64,
    file: String,
    line: i64,
    value: Value,
    kind: String,
    symbol: String,
}

/// What an argument's url is: its patterns, or the name of a parameter of the method it is written in - whether the url is a
/// `string.Format` of it, the text written before it, and the whole url as patterns.
enum Url {
    Pattern(Vec<String>),
    Param(String, bool, String, Vec<String>),
}

pub(super) fn load(conn: &Connection, csharp: &HashMap<String, String>) -> rusqlite::Result<Vec<Site>> {
    let functions = query(conn, "functions", &["id", "file", "line", "end_line", "qualname"])?;
    let mut holding: HashMap<&str, Vec<&Vec<String>>> = HashMap::new();
    for f in &functions {
        holding.entry(f[1].as_str()).or_default().push(f);
    }
    let mut qualnames: HashMap<&str, Vec<&str>> = HashMap::new();
    for f in &functions {
        qualnames.entry(f[4].as_str()).or_default().push(f[0].as_str());
    }
    let calls: Vec<Call> = query(conn, "calls", &["id", "file", "line", "end_line", "symbol"])?
        .into_iter()
        .filter(|c| csharp.contains_key(&c[1]))
        .map(|c| {
            let (line, end) = (c[2].parse().unwrap_or(0), c[3].parse().unwrap_or(0));
            let inner = holding.get(c[1].as_str()).and_then(|fs| {
                fs.iter()
                    .filter(|f| f[2].parse::<i64>().unwrap_or(0) <= line && line <= f[3].parse::<i64>().unwrap_or(0))
                    .max_by_key(|f| f[2].parse::<i64>().unwrap_or(0))
            });
            let (member, qualname, start) =
                inner.map(|f| (f[0].clone(), f[4].clone(), f[2].parse().unwrap_or(0))).unwrap_or_default();
            Call { id: c[0].clone(), file: c[1].clone(), line, end, symbol: c[4].clone(), member, qualname, start }
        })
        .collect();
    let args = arguments(conn)?;
    let declared = query(conn, "parameters", &["qualname", "name", "position"])?;
    let params: HashSet<(String, String)> = declared.iter().map(|p| (p[0].clone(), p[1].clone())).collect();
    let position: HashMap<(&str, &str), i64> = declared.iter().map(|p| ((p[0].as_str(), p[1].as_str()), p[2].parse().unwrap_or(-1))).collect();
    let bases = bases(conn)?;
    let locals = query(conn, "locals", &["file", "line", "name"])?;
    let none = Vec::new();
    let of = |c: &Call| args.get(&c.id).unwrap_or(&none);
    // AN ARGUMENT BY THE PARAMETER IT BINDS; where Roslyn bound no overload, by its place; for a request message, also the
    // member its object initializer sets (`new HttpRequestMessage { RequestUri = .. }`, `Method`), inside the construction.
    let pick = |c: &Call, name: &str| -> Option<&Arg> {
        let place = [("method", 0), ("requestUri", 1)].iter().find(|(n, _)| *n == name).map(|(_, p)| *p);
        let member = name[..1].to_uppercase() + &name[1..];
        of(c).iter().find(|a| a.name == name).or_else(|| {
            let message = MESSAGE.contains(&c.symbol.as_str());
            message
                .then(|| of(c).iter().find(|a| a.name.is_empty() && Some(a.position) == place))
                .flatten()
                .or_else(|| {
                    let set = message.then(|| args.get("")).flatten()?;
                    set.iter().find(|a| a.name == member && a.file == c.file && c.line <= a.line && a.line <= c.end)
                })
        })
    };
    let value_of = |a: &Arg, qualname: &str| -> Option<Url> {
        match (&a.value, a.kind.as_str()) {
            (Value::String(s), "const" | "folded") => Some(Url::Pattern(vec![s.clone()])),
            // A BASE THE RUN TIME KNOWS AND THE CALLER'S URL (`_cfg.Url + url`, `string.Concat(_base, endpoint)`): holes alone, each
            // string ending in this method's parameter - the method is a wrapper of it, as a bare parameter makes it.
            (Value::Array(alternatives), _) if handed(alternatives).is_some_and(|(p, _, _)| params.contains(&(qualname.to_string(), p.clone()))) =>
                handed(alternatives).map(|(p, formats, prefix)| Url::Param(p, formats, prefix, folded(alternatives))),
            (Value::Array(alternatives), _) => Some(Url::Pattern(folded(alternatives))),
            // `string.Empty`: the base address alone, as `""` is.
            (_, "symbol") if a.symbol == "System.String.Empty" => Some(Url::Pattern(vec![String::new()])),
            (_, "symbol") if params.contains(&(qualname.to_string(), a.symbol.clone())) => Some(Url::Param(a.symbol.clone(), false, String::new(), vec!["{}".into()])),
            _ => None,
        }
    };
    let url_of = |c: &Call, name: &str| -> Option<Url> {
        let a = pick(c, name)?;
        // A URL WRAPPED IN `new Uri(url, UriKind..)` is the string the `Uri` is made of.
        let wrapped = || {
            let uri = calls.iter().find(|u| u.symbol == URI && u.file == a.file && u.line == a.line && u.member == c.member)?;
            of(uri).iter().find(|u| u.name == "uriString" || (u.name.is_empty() && u.position == 0))
        };
        Some(value_of(a, &c.qualname).or_else(|| value_of(wrapped()?, &c.qualname)).unwrap_or_else(|| Url::Pattern(vec!["*".into()])))
    };
    // A WRAPPER CALLED THROUGH ITS INTERFACE binds the interface's parameter names: its url is the argument of the
    // wrapper's parameter name, else the one at its place.
    let url_for = |c: &Call, p: &str, at: i64| -> Option<Url> {
        if of(c).iter().any(|a| a.name == p) {
            return url_of(c, p);
        }
        let a = of(c).iter().find(|a| a.position == at)?;
        Some(value_of(a, &c.qualname).unwrap_or_else(|| Url::Pattern(vec!["*".into()])))
    };
    // THE REQUEST A `Send` IS HANDED: the `HttpRequestMessage` built in its argument, or with the local it names.
    let message = |c: &Call| -> Option<&Call> {
        let a = pick(c, "request")?;
        // The local's declaration is the last of that name in the same method before the call.
        let declared = locals
            .iter()
            .filter(|l| a.kind == "symbol" && l[0] == c.file && l[2] == a.symbol)
            .filter_map(|l| l[1].parse::<i64>().ok())
            .filter(|line| c.start <= *line && *line <= c.line)
            .max();
        calls.iter().find(|m| {
            MESSAGE.contains(&m.symbol.as_str())
                && m.member == c.member
                && match declared {
                    Some(line) => m.line == line,
                    None => c.line <= m.line && m.line <= c.end,
                }
        })
    };
    // THE PRIMITIVES: each call's verb and the url it asks for.
    let mut asked: HashMap<usize, (String, Url)> = HashMap::new();
    for (n, c) in calls.iter().enumerate() {
        let Some(name) = CLASSES.iter().find_map(|k| c.symbol.strip_prefix(k)) else { continue };
        let name = name.split('<').next().unwrap_or(name);
        if name.starts_with("Send") {
            let Some(m) = message(c) else { continue };
            let verb = pick(m, "method").and_then(|a| a.symbol.strip_prefix(METHOD)).map(str::to_uppercase);
            if let Some(url) = url_of(m, "requestUri") {
                asked.insert(n, (verb.unwrap_or_else(|| "ANY".into()), url));
            }
        } else if let Some((_, verb)) = VERBS.iter().find(|(prefix, _)| name.starts_with(prefix)) {
            if let Some(url) = url_of(c, "requestUri") {
                asked.insert(n, (verb.to_string(), url));
            }
        }
    }
    // THE WRAPPERS, to a fixed point: a method whose request's url is its own parameter, by qualname -> (id, parameter, verb,
    // whether it formats the url, its prefix).
    let unique = |q: &str| qualnames.get(q).filter(|ids| ids.len() == 1).map(|ids| ids[0].to_string());
    let mut wrappers: HashMap<String, Wrapper> = HashMap::new();
    let mut primitives: Vec<_> = asked.iter().collect();
    primitives.sort_by_key(|(n, _)| **n);
    for (n, (verb, url)) in primitives {
        if let (Url::Param(p, formats, prefix, _), Some(id)) = (url, unique(&calls[*n].qualname)) {
            let at = position.get(&(calls[*n].qualname.as_str(), p.as_str())).copied().unwrap_or(-1);
            let wrapper = wrappers.entry(calls[*n].qualname.clone()).or_insert((id, p.clone(), verb.clone(), *formats, prefix.clone(), at));
            if wrapper.2 != *verb {
                wrapper.2 = "ANY".into();
            }
        }
    }
    let mut aliased: HashMap<String, String> = HashMap::new();
    loop {
        // EACH WRAPPER IS ALSO THE INTERFACE OR BASE METHOD IT IMPLEMENTS: a call through `IClient` is bound to that one.
        for (alias, of) in aliases(&wrappers, &bases) {
            if let Some(entry) = wrappers.get(&of).cloned() {
                aliased.insert(alias.clone(), of);
                wrappers.entry(alias).or_insert(entry);
            }
        }
        let mut found = vec![];
        for c in &calls {
            let Some((_, p, verb, formats, prefix, at)) = wrappers.get(c.symbol.split('<').next().unwrap_or("")) else { continue };
            if let (Some(Url::Param(own, also, before, _)), Some(id)) = (url_for(c, p, *at), unique(&c.qualname)) {
                if !wrappers.contains_key(&c.qualname) {
                    let own_at = position.get(&(c.qualname.as_str(), own.as_str())).copied().unwrap_or(-1);
                    found.push((c.qualname.clone(), (id, own, verb.clone(), *formats || also, format!("{prefix}{before}"), own_at)));
                }
            }
        }
        if found.is_empty() {
            break;
        }
        wrappers.extend(found);
    }
    // THE ROWS: a primitive nobody parameterises, and every call of a wrapper that hands it a url of its own.
    let mut called: HashSet<&str> = calls.iter().map(|c| c.symbol.split('<').next().unwrap_or("")).collect();
    // A WRAPPER CALLED THROUGH ITS INTERFACE is called.
    let through: Vec<&str> = aliased.iter().filter(|(alias, _)| called.contains(alias.as_str())).map(|(_, of)| of.as_str()).collect();
    called.extend(through);
    let uncalled = |c: &Call, prefix: &str| !prefix.is_empty() && !called.contains(c.qualname.as_str());
    let paths = super::files_of(conn, "csharp")?;
    let mut sites = vec![];
    for (n, c) in calls.iter().enumerate() {
        let (verb, via, urls): (_, _, Vec<String>) = match (asked.get(&n), wrappers.get(c.symbol.split('<').next().unwrap_or(""))) {
            (Some((verb, Url::Pattern(urls))), _) => (verb.clone(), String::new(), urls.clone()),
            (Some((verb, Url::Param(_, _, prefix, whole))), _) if uncalled(c, prefix) => (verb.clone(), String::new(), whole.clone()),
            (None, Some((id, p, verb, formats, prefix, at))) => match url_for(c, p, *at) {
                Some(Url::Pattern(urls)) if *formats => (verb.clone(), id.clone(), urls.iter().map(|u| format!("{prefix}{}", formatted(u))).collect()),
                Some(Url::Pattern(urls)) => (verb.clone(), id.clone(), urls.iter().map(|u| format!("{prefix}{u}")).collect()),
                Some(Url::Param(_, _, before, whole)) if uncalled(c, &format!("{prefix}{before}")) => (verb.clone(), id.clone(), whole.iter().map(|w| format!("{prefix}{w}")).collect()),
                _ => continue,
            },
            _ => continue,
        };
        // ONE ROW PER PATH: the query cut, two alternatives naming one path are one row.
        let paths_asked: std::collections::BTreeSet<String> = urls.iter().map(|u| u.split('?').next().unwrap_or("").to_string()).collect();
        for url in paths_asked {
            sites.push(Site {
                file: paths.get(&c.file).cloned().unwrap_or_default(),
                call: c.id.clone(),
                member: c.member.clone(),
                line: c.line,
                verb: verb.clone(),
                url,
                via: via.clone(),
            });
        }
    }
    sites.sort_by(|a, b| (&a.file, a.line, &a.call).cmp(&(&b.file, b.line, &b.call)));
    Ok(sites)
}

/// A wrapper: its `functions` id, its url parameter, its verb, whether it formats the url, its prefix, and the url
/// parameter's place.
type Wrapper = (String, String, String, bool, String, i64);

/// Every class's bases by its symbol, type arguments left out of both: `Demo.Client` -> [`Demo.IClient`].
fn bases(conn: &Connection) -> rusqlite::Result<HashMap<String, Vec<String>>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    if !crate::rows::dups::has_columns(conn, "classes", &["symbol", "bases"]) {
        return Ok(out);
    }
    let mut list = Column::new(conn, "classes", "bases")?;
    let sql = format!("SELECT k.symbol, {}, {} FROM classes k", crate::rows::dups::owner_sql(conn, "classes", "k"), scalar_sql(conn, "classes", "k", "bases"));
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], |r| Ok((text(r, 0)?, text(r, 1)?, r.get::<_, SqlValue>(2)?)))?;
    for row in rows {
        let (symbol, owner, cell) = row?;
        let bare = |s: &str| s.split('<').next().unwrap_or(s).to_string();
        out.entry(bare(&symbol)).or_default().extend(list.strings(&owner, &cell)?.iter().map(|b| bare(b)));
    }
    Ok(out)
}

/// The interface and base methods each wrapper implements, as (alias, wrapper): `Demo.Client.Post` is also
/// `Demo.IClient.Post` - through every base above it (a visited set ends bases that name each other).
fn aliases(wrappers: &HashMap<String, Wrapper>, bases: &HashMap<String, Vec<String>>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut names: Vec<&String> = wrappers.keys().collect();
    names.sort();
    for qualname in names {
        let Some((class, method)) = qualname.rsplit_once('.') else { continue };
        let mut seen: HashSet<&str> = HashSet::new();
        let mut todo: Vec<&str> = vec![class];
        while let Some(at) = todo.pop() {
            for base in bases.get(at).into_iter().flatten() {
                if seen.insert(base.as_str()) {
                    out.push((format!("{base}.{method}"), qualname.clone()));
                    todo.push(base.as_str());
                }
            }
        }
    }
    out
}

/// The one parameter every alternative of a template ends in (its hole's `symbol`), whether it is a `string.Format`'s format,
/// and the text before it - the holes it starts with left out, each later hole `{}` - all alike in every alternative; or None.
fn handed(alternatives: &[Value]) -> Option<(String, bool, String)> {
    let mut named: Option<(String, bool, String)> = None;
    for alt in alternatives {
        let (hole, before) = alt.as_array()?.split_last()?;
        let prefix: String = before
            .iter()
            .skip_while(|p| p.is_object())
            .map(|p| p.as_str().map(str::to_string).unwrap_or_else(|| "{}".into()))
            .collect();
        let last = (hole.get("symbol")?.as_str()?.to_string(), hole.get("format").is_some(), prefix);
        if named.as_ref().is_some_and(|n| *n != last) { return None; }
        named = Some(last);
    }
    named
}

/// A url a `string.Format` fills: each `{n}`, `{n,w}` or `{n:f}` is a hole `{}`, `{{` and `}}` the brace itself.
fn formatted(url: &str) -> String {
    let mut out = String::new();
    let mut rest = url;
    while let Some(c) = rest.chars().next() {
        let item = rest.strip_prefix('{').and_then(|r| r.find('}').map(|end| &r[..end]));
        let digits = item.map(|i| i.len() - i.trim_start_matches(|d: char| d.is_ascii_digit()).len()).unwrap_or(0);
        if rest.starts_with("{{") || rest.starts_with("}}") {
            out.push(c);
            rest = &rest[2..];
        } else if let (Some(item), true) = (item, digits > 0) {
            out.push_str("{}");
            rest = &rest[item.len() + 2..];
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

/// An interpolation's alternatives as patterns, each path once: each piece's text, a hole `{}`. An alternative of holes
/// alone (a ternary's arm only the run time knows) says nothing while another one says a path, and is left out.
fn folded(alternatives: &[Value]) -> Vec<String> {
    let all = spelled(alternatives);
    let said: Vec<String> = all.iter().filter(|p| p.replace("{}", "").chars().any(char::is_alphanumeric)).cloned().collect();
    if said.is_empty() { all } else { said }
}

fn spelled(alternatives: &[Value]) -> Vec<String> {
    let spelled: std::collections::BTreeSet<String> = alternatives
        .iter()
        .map(|alt| {
            alt.as_array()
                .map(|pieces| pieces.iter().map(|p| p.as_str().map(str::to_string).unwrap_or_else(|| "{}".into())).collect())
                .unwrap_or_default()
        })
        .map(|s: String| s.split('?').next().unwrap_or("").to_string())
        .collect();
    spelled.into_iter().collect()
}

/// Every argument of a call, by call id: the parameter it binds, its folded value or template, and the symbol it reads.
fn arguments(conn: &Connection) -> rusqlite::Result<HashMap<String, Vec<Arg>>> {
    if crate::rows::schema::existing_columns(conn, "arguments").unwrap_or_default().is_empty() {
        return Ok(HashMap::new());
    }
    let picked: Vec<String> =
        ["id", "call", "name", "const", "const_kind", "symbol", "template", "position", "file", "line"].iter().map(|c| scalar_sql(conn, "arguments", "a", c)).collect();
    let sql = format!("SELECT {} FROM arguments a", picked.join(", "));
    let mut templates = Column::new(conn, "arguments", "template")?;
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([])?;
    let mut out: HashMap<String, Vec<Arg>> = HashMap::new();
    while let Some(r) = rows.next()? {
        let kind = text(r, 4)?;
        let held = templates.value(&text(r, 0)?, &r.get::<_, SqlValue>(6)?)?;
        let value = match held {
            Value::Array(_) if !matches!(kind.as_str(), "const" | "folded") => held,
            _ => Value::String(text(r, 3)?),
        };
        let (position, line) = (text(r, 7)?.parse().unwrap_or(-1), text(r, 9)?.parse().unwrap_or(0));
        out.entry(text(r, 1)?).or_default().push(Arg { name: text(r, 2)?, position, file: text(r, 8)?, line, value, kind, symbol: text(r, 5)? });
    }
    Ok(out)
}

/// Every row of a table, the columns asked for as text - a column the table lacks is "" (the TypeScript half writes
/// `calls` and `functions` too, with columns of its own), a table no half wrote is no rows.
fn query(conn: &Connection, table: &str, columns: &[&str]) -> rusqlite::Result<Vec<Vec<String>>> {
    if crate::rows::schema::existing_columns(conn, table).unwrap_or_default().is_empty() {
        return Ok(Vec::new());
    }
    let picked: Vec<String> = columns.iter().map(|c| scalar_sql(conn, table, "t", c)).collect();
    let mut stmt = conn.prepare(&format!("SELECT {} FROM \"{table}\" t", picked.join(", ")))?;
    let rows = stmt.query_map([], |r| (0..columns.len()).map(|i| text(r, i)).collect::<rusqlite::Result<Vec<String>>>())?;
    rows.collect()
}
