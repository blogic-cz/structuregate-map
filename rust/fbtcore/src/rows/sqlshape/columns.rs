//! WHICH COLUMN OF WHICH TABLE a statement names, and where: `sql_column_refs`, read off the statement's tree.
//!
//! A column's SOURCE is found the way SQL finds it: its qualifier is an alias or a table name of the nearest query
//! (a SELECT's body) that has one, then of the queries around it - a correlated subquery reads its outer query's
//! tables - and of the statement itself for an UPDATE, DELETE, INSERT or MERGE target. An unqualified column belongs to
//! the only source of its query; with several it is NOT ATTRIBUTED - an empty `name`, never a guess, never dropped.
//! The left side of a SET, an INSERT's column list and a MERGE action's columns are the TARGET's.
//!
//! `role` is the clause the column is in (`select`, `where`, `join`, `groupby`, `having`, `orderby`, `set`, `insert`,
//! `output`, `merge_on`, `window`, ...): the nearest clause above it, and `window` anywhere inside an OVER.

use super::Node;
use std::collections::HashMap;

/// One column a statement names.
#[derive(Clone, Debug, PartialEq)]
pub struct ColumnRef {
    pub line: i64,
    pub role: String,
    pub schema: String,
    pub name: String,
    pub source_kind: String,
    pub alias: String,
    pub column: String,
}

/// A table, CTE, derived table, temp or variable a query reads from.
#[derive(Clone, Debug)]
struct Source {
    kind: &'static str,
    schema: String,
    name: String,
    alias: String,
}

struct Tree<'a> {
    nodes: &'a [Node],
    parent: Vec<Option<usize>>,
    children: Vec<Vec<usize>>,
}

impl<'a> Tree<'a> {
    fn new(nodes: &'a [Node]) -> Self {
        let at: HashMap<i64, usize> = nodes.iter().enumerate().map(|(i, node)| (node.n, i)).collect();
        let parent: Vec<Option<usize>> = nodes.iter().map(|node| node.parent.and_then(|p| at.get(&p).copied())).collect();
        let mut children = vec![Vec::new(); nodes.len()];
        for (i, p) in parent.iter().enumerate() {
            if let Some(p) = p {
                children[*p].push(i);
            }
        }
        Tree { nodes, parent, children }
    }

    fn kind(&self, i: usize) -> &str {
        &self.nodes[i].kind
    }

    fn child(&self, i: usize, kind: &str) -> Option<&str> {
        self.children[i].iter().find(|&&c| self.kind(c) == kind).map(|&c| self.nodes[c].value.as_str())
    }

    /// Every node above `i`, nearest first.
    fn ancestors(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        std::iter::successors(self.parent[i], move |&p| self.parent[p])
    }

    /// The scope a node belongs to: its nearest `query`, or the root.
    fn scope(&self, i: usize) -> usize {
        self.ancestors(i).find(|&a| self.kind(a) == "query" || self.parent[a].is_none()).unwrap_or(0)
    }
}

/// The column refs of one statement's tree. `default_schema` is what an unqualified table's schema is.
pub fn of(nodes: &[Node], default_schema: &str) -> Vec<ColumnRef> {
    if nodes.is_empty() {
        return Vec::new();
    }
    let tree = Tree::new(nodes);
    let ctes: Vec<String> = (0..nodes.len()).filter(|&i| tree.kind(i) == "cte").map(|i| nodes[i].value.to_lowercase()).collect();
    // EVERY SOURCE, filed under its scope; the target of an UPDATE, DELETE, INSERT or MERGE noted on its own.
    let mut sources: HashMap<usize, Vec<Source>> = HashMap::new();
    let mut target: Option<Source> = None;
    for i in 0..nodes.len() {
        let source = match tree.kind(i) {
            "table" => {
                let name = nodes[i].value.clone();
                let spelled = tree.child(i, "schema");
                let kind = if name.starts_with('#') {
                    "temp"
                } else if name.starts_with('@') {
                    "variable"
                } else if spelled.is_none() && ctes.contains(&name.to_lowercase()) {
                    "cte"
                } else if spelled.is_none() && matches!(name.to_lowercase().as_str(), "inserted" | "deleted") {
                    "pseudo"
                } else {
                    "table"
                };
                let schema = if kind == "table" { spelled.unwrap_or(default_schema).to_string() } else { String::new() };
                Source { kind, schema, name, alias: tree.child(i, "alias").unwrap_or("").to_string() }
            }
            "subquery" => match tree.child(i, "alias") {
                Some(alias) => Source { kind: "derived", schema: String::new(), name: String::new(), alias: alias.to_string() },
                None => continue,
            },
            _ => continue,
        };
        if tree.parent[i].is_some_and(|p| tree.kind(p) == "target") {
            target = Some(source.clone());
        }
        sources.entry(tree.scope(i)).or_default().push(source);
    }
    // `UPDATE o SET ... FROM dbo.Orders o`: the target is an ALIAS, and the table is the one the FROM gives it.
    if let Some(alias) = target.as_ref().filter(|t| t.kind == "table" && t.alias.is_empty()).map(|t| t.name.clone())
        && let Some(own) = sources.get_mut(&0)
        && let Some(real) = own.iter().find(|s| s.alias.eq_ignore_ascii_case(&alias)).cloned()
    {
        own.retain(|s| !(s.alias.is_empty() && s.name.eq_ignore_ascii_case(&alias)));
        target = Some(real);
    }
    let mut refs = Vec::new();
    for i in 0..nodes.len() {
        if tree.kind(i) != "column" {
            continue;
        }
        let up = tree.parent[i];
        // `COUNT(*)` counts rows: no column is named.
        if nodes[i].value == "*" && up.is_some_and(|p| tree.kind(p) == "func") {
            continue;
        }
        let parts: Vec<&str> = nodes[i].value.split('.').collect();
        let column = parts[parts.len() - 1].to_string();
        let qualifier = parts[..parts.len() - 1].to_vec();
        let role = role_of(&tree, i);
        // A SELECT-LIST ALIAS named again in ORDER BY is no column of a table.
        if qualifier.is_empty() && role == "orderby" && aliased(&tree, tree.scope(i), &column) {
            continue;
        }
        let written = up.is_some_and(|p| {
            (tree.kind(p) == "set" && tree.children[p].first() == Some(&i)) || tree.kind(p) == "insert" || tree.parent[p].is_none()
        });
        let found = if written && qualifier.is_empty() && target.is_some() {
            target.clone()
        } else {
            resolve(&tree, &sources, i, &qualifier)
        };
        let (schema, name, source_kind, alias) = match found {
            Some(s) => (s.schema, s.name, s.kind.to_string(), s.alias),
            None => (String::new(), String::new(), String::new(), qualifier.last().map(|q| q.to_string()).unwrap_or_default()),
        };
        refs.push(ColumnRef { line: nodes[i].line, role, schema, name, source_kind, alias, column });
    }
    refs
}

/// The source a qualified column names, or an unqualified one's only source - nearest scope first.
fn resolve(tree: &Tree, sources: &HashMap<usize, Vec<Source>>, i: usize, qualifier: &[&str]) -> Option<Source> {
    let mut scopes: Vec<usize> = tree.ancestors(i).filter(|&a| tree.kind(a) == "query").collect();
    scopes.push(0);
    for scope in scopes {
        let Some(here) = sources.get(&scope) else { continue };
        if qualifier.is_empty() {
            return match here.as_slice() {
                [only] => Some(only.clone()),
                _ => None,
            };
        }
        let last = qualifier[qualifier.len() - 1];
        let schema = qualifier.len().checked_sub(2).map(|s| qualifier[s]);
        let hit = here.iter().find(|s| s.alias.eq_ignore_ascii_case(last)).or_else(|| {
            here.iter().find(|s| {
                s.alias.is_empty() && s.name.eq_ignore_ascii_case(last) && schema.is_none_or(|sc| s.schema.eq_ignore_ascii_case(sc))
            })
        });
        if let Some(hit) = hit {
            return Some(hit.clone());
        }
    }
    None
}

/// Whether a query's select list names an item `name`.
fn aliased(tree: &Tree, scope: usize, name: &str) -> bool {
    (0..tree.nodes.len()).any(|i| {
        tree.kind(i) == "alias"
            && tree.nodes[i].value.eq_ignore_ascii_case(name)
            && tree.parent[i].is_some_and(|p| tree.kind(p) == "item" && tree.scope(p) == scope)
    })
}

/// The clause a column is in: the nearest one above it, `window` anywhere inside an OVER of its own query.
fn role_of(tree: &Tree, i: usize) -> String {
    let mut role: Option<&str> = None;
    for a in tree.ancestors(i) {
        let kind = tree.kind(a);
        if kind == "over" || kind == "partition" {
            return "window".into();
        }
        // OUTPUT / RETURNING lists items too: they are what the statement hands back, not what it selects.
        if kind == "output" {
            return "output".into();
        }
        if kind == "query" || tree.parent[a].is_none() {
            if role.is_none() && tree.parent[a].is_none() {
                role = Some(match kind {
                    "insert" => "insert",
                    "merge" => "merge",
                    _ => "",
                });
            }
            break;
        }
        if role.is_none() {
            role = match kind {
                "item" => Some("select"),
                "where" => Some("where"),
                "on" => Some(if tree.parent[a].is_some_and(|p| tree.parent[p].is_none() && tree.kind(p) == "merge") { "merge_on" } else { "join" }),
                "groupby" => Some("groupby"),
                "having" => Some("having"),
                "orderby" => Some("orderby"),
                "set" => Some("set"),
                "output" => Some("output"),
                "values" => Some("values"),
                "top" => Some("top"),
                "insert" => Some("insert"),
                "using" => Some("using"),
                _ => None,
            };
        }
    }
    role.unwrap_or("").to_string()
}

#[cfg(test)]
mod tests {
    use super::super::Node;

    fn tree(spec: &[(i64, Option<i64>, &str, &str)]) -> Vec<Node> {
        spec.iter().map(|&(n, parent, kind, value)| Node { n, parent, kind: kind.into(), value: value.into(), line: 1 }).collect()
    }

    #[test]
    fn an_alias_is_its_table_and_an_unqualified_column_of_two_tables_is_nobodys() {
        let nodes = tree(&[
            (0, None, "select", ""), (1, Some(0), "query", ""), (2, Some(1), "item", ""), (3, Some(2), "column", "o.Amount"),
            (4, Some(1), "from", ""), (5, Some(4), "join", "left"), (6, Some(5), "table", "Orders"), (7, Some(6), "alias", "o"),
            (8, Some(5), "table", "Customers"), (9, Some(8), "schema", "Sales"), (10, Some(1), "where", ""), (11, Some(10), "column", "X"),
        ]);
        let refs = super::of(&nodes, "dbo");
        assert_eq!(refs.len(), 2);
        assert_eq!((refs[0].schema.as_str(), refs[0].name.as_str(), refs[0].role.as_str()), ("dbo", "Orders", "select"));
        assert_eq!((refs[1].name.as_str(), refs[1].column.as_str(), refs[1].role.as_str()), ("", "X", "where"));
    }

    #[test]
    fn a_correlated_column_reads_the_outer_query() {
        let nodes = tree(&[
            (0, None, "select", ""), (1, Some(0), "query", ""), (2, Some(1), "from", ""), (3, Some(2), "table", "Customers"),
            (4, Some(3), "alias", "c"), (5, Some(1), "where", ""), (6, Some(5), "exists", ""), (7, Some(6), "subquery", ""),
            (8, Some(7), "query", ""), (9, Some(8), "from", ""), (10, Some(9), "table", "Orders"), (11, Some(8), "where", ""),
            (12, Some(11), "column", "c.Id"), (13, Some(11), "column", "Total"),
        ]);
        let refs = super::of(&nodes, "dbo");
        assert_eq!(refs[0].name, "Customers");
        assert_eq!(refs[1].name, "Orders");
    }
}
