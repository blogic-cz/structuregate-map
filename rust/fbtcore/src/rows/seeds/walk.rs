//! THE CHAIN, WALKED AS SQLCMD RUNS IT: a file with no `:r` pointing at it is a root, and every `:r` is
//! spliced in at its line, so a variable `PostDeployment.sql` declares is set when the `Products.sql` it
//! runs next reads it, and a `SET @currentTierID = 5` holds for the INSERT below it and no further. A
//! `#temp`'s or `@table`'s rows wait until a MERGE or INSERT ... SELECT moves them into a table.

use super::tables::Tables;
use super::value::{same_number, shown, Val};
use super::{schema, Files, Pending, Seed, Step};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Default)]
struct Temp {
    columns: Vec<String>,
    rows: Vec<Pending>,
}

pub struct Walker<'a> {
    pub steps: &'a HashMap<String, Vec<Step>>,
    pub files: &'a Files,
    pub tables: &'a Tables,
    pub seeds: Vec<Seed>,
    seen: HashSet<String>,
    pub flushed: HashSet<String>,
    /// Rows that went into a temp, by step - kept to name the ones that never left it.
    pub stranded: BTreeMap<String, (Pending, String)>,
}

/// One root's run: its variables, its temps, the files it is inside, and the seeds it wrote.
struct Run {
    root: String,
    variables: HashMap<String, Val>,
    temps: HashMap<String, Temp>,
    running: HashSet<String>,
    mine: Vec<usize>,
}

impl<'a> Walker<'a> {
    pub fn new(steps: &'a HashMap<String, Vec<Step>>, files: &'a Files, tables: &'a Tables) -> Self {
        Walker { steps, files, tables, seeds: Vec::new(), seen: HashSet::new(), flushed: HashSet::new(), stranded: BTreeMap::new() }
    }

    pub fn root(&mut self, root: &str) {
        let mut run = Run { root: root.into(), variables: HashMap::new(), temps: HashMap::new(), running: HashSet::new(), mine: Vec::new() };
        self.walk(&mut run, root, &[]);
    }

    fn emit(&mut self, run: &mut Run, row: Pending, step_schema: &str, name: &str) {
        self.flushed.insert(row.step.id.clone());
        let schema = schema(step_schema);
        let seed = self.tables.seed(self.tables.completed(&row, &schema, name), &schema, name);
        let key = format!("{}|{}.{}|{}{}", row.step.id, seed.schema, seed.name, super::value::columns(&seed.row.columns), super::value::values(&seed.row.values));
        if !self.seen.insert(key) {
            return;
        }
        run.mine.push(self.seeds.len());
        self.seeds.push(seed);
    }

    fn walk(&mut self, run: &mut Run, file: &str, outer: &[String]) {
        if !run.running.insert(file.to_string()) {
            return;
        }
        let database = self.files.get(file).map(|f| f.database.clone()).unwrap_or_default();
        let path = self.files.get(file).map(|f| f.path.clone()).unwrap_or_default();
        let mut branches: Vec<(i64, String)> = Vec::new();
        let steps = self.steps.get(file).cloned().unwrap_or_default();
        for step in steps {
            branches.retain(|b| b.0 >= step.line);
            let within: Vec<String> = outer.iter().cloned().chain(branches.iter().map(|b| b.1.clone())).collect();
            let condition = within.join(" AND ");
            match step.action.as_str() {
                "include" => self.walk(run, &step.name, &within),
                "if" => branches.push((step.source.parse::<i64>().unwrap_or(step.line), step.name.clone())),
                "set" => {
                    let value = known(&run.variables, step.kinds.first().map_or("expr", String::as_str), step.exprs.first().map_or("", String::as_str));
                    run.variables.insert(step.name.to_lowercase(), value);
                }
                "temp" => {
                    let columns = if !step.columns.is_empty() { step.columns.clone() } else { self.tables.columns_of(&database, &step.source, false) };
                    run.temps.insert(step.name.to_lowercase(), Temp { columns, rows: Vec::new() });
                }
                "drop" => {
                    run.temps.remove(&step.name.to_lowercase());
                }
                "values" => {
                    let values: Vec<Val> = step.kinds.iter().enumerate()
                        .map(|(i, k)| known(&run.variables, k, step.exprs.get(i).map_or("", String::as_str))).collect();
                    if !temporary(&step.name) {
                        let row = Pending::new(&step, &run.root, &database, &condition, step.columns.clone(), values, Vec::new());
                        let (schema, name) = (step.schema.clone(), step.name.clone());
                        self.emit(run, row, &schema, &name);
                        continue;
                    }
                    let into = run.temps.entry(step.name.to_lowercase()).or_default();
                    let columns = if !step.columns.is_empty() { step.columns.clone() } else { into.columns.clone() };
                    let row = Pending::new(&step, &run.root, &database, &condition, columns, values, vec![step.name.clone()]);
                    into.rows.push(row.clone());
                    self.stranded.entry(step.id.clone()).or_insert((row, step.name.clone()));
                }
                "flow" => {
                    let Some(source) = run.temps.get(&step.source.to_lowercase()) else { continue };
                    let into = if temporary(&step.name) {
                        Vec::new()
                    } else {
                        self.tables.columns_of(&database, &format!("{}.{}", schema(&step.schema), step.name), true)
                    };
                    for moved in moved(source, &step, &into) {
                        if !temporary(&step.name) {
                            let (schema, name) = (step.schema.clone(), step.name.clone());
                            self.emit(run, moved, &schema, &name);
                            continue;
                        }
                        let mut via = moved.via.clone();
                        via.push(step.name.clone());
                        run.temps.entry(step.name.to_lowercase()).or_default().rows.push(Pending { via, ..moved });
                    }
                }
                "update" => {
                    let Some(change) = Change::of(&step, &run.variables, &format!("{path}:{}", step.line)) else { continue };
                    if temporary(&step.name) {
                        if let Some(held) = run.temps.get_mut(&step.name.to_lowercase()) {
                            for row in held.rows.iter_mut() {
                                if let Some(updated) = change.apply(row) {
                                    *row = updated;
                                }
                            }
                        }
                        continue;
                    }
                    let target = format!("{}.{}", schema(&step.schema), step.name);
                    for &at in &run.mine {
                        let seed = &self.seeds[at];
                        if !format!("{}.{}", seed.schema, seed.name).eq_ignore_ascii_case(&target)
                            || !seed.row.database.eq_ignore_ascii_case(&database)
                        {
                            continue;
                        }
                        if let Some(updated) = change.apply(&seed.row) {
                            let (schema, name) = (seed.schema.clone(), seed.name.clone());
                            self.seeds[at] = self.tables.seed(updated, &schema, &name);
                        }
                    }
                }
                _ => {}
            }
        }
        run.running.remove(file);
    }
}

/// An UPDATE as the walk applies it: the rows whose WHERE columns hold one of the listed values (every
/// column must match, AND) get the SET values. A row it leaves as it was carries no mark - a seed script may
/// `UPDATE` every row it just inserted, with the same values.
struct Change {
    sets: Vec<(String, Val)>,
    wheres: Vec<(String, Val)>,
    at: String,
}

impl Change {
    fn of(step: &Step, variables: &HashMap<String, Val>, at: &str) -> Option<Change> {
        let cells: Vec<Val> = step.kinds.iter().enumerate()
            .map(|(i, k)| known(variables, k, step.exprs.get(i).map_or("", String::as_str))).collect();
        let sets = step.columns.iter().enumerate().map(|(i, c)| (c.clone(), cells[i].clone())).collect();
        let wheres: Vec<(String, Val)> =
            step.targets.iter().enumerate().map(|(i, c)| (c.clone(), cells[step.columns.len() + i].clone())).collect();
        // A WHERE value the chain does not know names no row: nothing is changed on a guess.
        if wheres.iter().any(|w| w.1.0 == "expr" || w.1.0 == "unset") {
            return None;
        }
        Some(Change { sets, wheres, at: at.into() })
    }

    fn apply(&self, row: &Pending) -> Option<Pending> {
        let mut grouped: Vec<(String, Vec<&Val>)> = Vec::new();
        for (column, value) in &self.wheres {
            match grouped.iter_mut().find(|g| g.0.eq_ignore_ascii_case(column)) {
                Some(group) => group.1.push(value),
                None => grouped.push((column.clone(), vec![value])),
            }
        }
        for (column, wanted) in &grouped {
            let at = row.columns.iter().position(|c| c.eq_ignore_ascii_case(column))?;
            if at >= row.values.len() || !wanted.iter().any(|w| same(w, &row.values[at])) {
                return None;
            }
        }
        let mut columns = row.columns.clone();
        let mut values = row.values.clone();
        let mut changed = false;
        for (column, value) in &self.sets {
            match columns.iter().position(|c| c.eq_ignore_ascii_case(column)) {
                None => {
                    columns.push(column.clone());
                    values.push(value.clone());
                    changed = true;
                }
                Some(at) => {
                    if at < values.len() && same(&values[at], value) && values[at].0 == value.0 {
                        continue;
                    }
                    if at < values.len() {
                        values[at] = value.clone();
                    }
                    changed = true;
                }
            }
        }
        if !changed {
            return None;
        }
        let mut updates = row.updates.clone();
        updates.push(self.at.clone());
        Some(Pending { columns, values, updates, ..row.clone() })
    }
}

/// SQL equality over what a script spells: numbers by value, strings case-insensitively (the default
/// collation), and NULL equal to nothing.
fn same(a: &Val, b: &Val) -> bool {
    let number = |k: &str| k == "int" || k == "num";
    let string = |k: &str| k == "str" || k == "vstr";
    if number(&a.0) && number(&b.0) {
        return same_number(&a.1, &b.1);
    }
    string(&a.0) && string(&b.0) && a.1.to_lowercase() == b.1.to_lowercase()
}

/// A value as the chain knows it at this step: a variable is what it was last set to, and a sum is added up
/// when every term is a whole number, or joined when every term is a string.
fn known(variables: &HashMap<String, Val>, kind: &str, expr: &str) -> Val {
    let variable = |name: &str| variables.get(&name.to_lowercase()).cloned().unwrap_or(("unset".into(), name.into()));
    if kind == "var" {
        return variable(expr);
    }
    if kind != "sum" {
        return (kind.into(), expr.into());
    }
    let terms: Vec<&str> = expr.split('\u{1f}').filter(|t| !t.is_empty()).collect();
    // A TERM IS ITS SIGN AND ITS BODY; a variable keeps its `@`, as the SET that named it did.
    let term_value = |t: &str| -> Val {
        let body = &t[1..];
        if body.starts_with('@') {
            variable(body)
        } else if let Some(text) = body.strip_prefix('\'') {
            ("str".into(), text.into())
        } else {
            ("int".into(), body.into())
        }
    };
    let known: Vec<Val> = terms.iter().map(|t| term_value(t)).collect();
    if known.iter().all(|k| k.0 == "str" || k.0 == "vstr") && terms.iter().all(|t| t.starts_with('+')) {
        let kind = if known.iter().any(|k| k.0 == "vstr") { "vstr" } else { "str" };
        return (kind.into(), known.iter().map(|k| k.1.as_str()).collect());
    }
    let mut total: i64 = 0;
    for (i, term) in terms.iter().enumerate() {
        let whole = if known[i].0 == "int" { known[i].1.parse::<i64>().ok() } else { None };
        let Some(whole) = whole else {
            let spelled: String = terms.iter().enumerate().map(|(n, t)| {
                let body = &t[1..];
                let op = if n == 0 && t.starts_with('+') { "" } else if t.starts_with('+') { " + " } else { " - " };
                let shown = match body.strip_prefix('\'') { Some(s) => format!("'{s}'"), None => body.to_string() };
                format!("{op}{shown}")
            }).collect();
            return ("expr".into(), spelled);
        };
        total += if term.starts_with('-') { -whole } else { whole };
    }
    ("int".into(), total.to_string())
}

fn temporary(name: &str) -> bool {
    name.starts_with('#') || name.starts_with('@')
}

/// A temp's rows as the flow writes them: each column renamed to the one it lands in, and a column the flow
/// does not carry dropped into `dropped` - with its value, since it is still a fact.
fn moved(source: &Temp, flow: &Step, into: &[String]) -> Vec<Pending> {
    // `SELECT *` carries every column: by position into a column list of the same length, else by name.
    let star = flow.columns.len() == 1 && flow.columns[0] == "*";
    let from: Vec<String> = if !star {
        flow.columns.clone()
    } else if !flow.targets.is_empty() && source.columns.len() == flow.targets.len() {
        source.columns.clone()
    } else {
        Vec::new()
    };
    let mut map: Vec<(String, String)> = Vec::new();
    for i in 0..from.len().min(flow.targets.len()) {
        let mut target = flow.targets[i].clone();
        // `#3`: the target's third writable column (a MERGE's INSERT with no column list).
        if let Some(position) = target.strip_prefix('#').and_then(|p| p.parse::<usize>().ok())
            && position >= 1
            && position <= into.len()
        {
            target = into[position - 1].clone();
        }
        if !map.iter().any(|(k, _)| k.eq_ignore_ascii_case(&from[i])) {
            map.push((from[i].clone(), target));
        }
    }
    source.rows.iter().map(|row| {
        let mut columns = Vec::new();
        let mut values = Vec::new();
        let mut dropped = row.dropped.clone();
        for i in 0..row.columns.len().min(row.values.len()) {
            let landed = map.iter().find(|(k, _)| k.eq_ignore_ascii_case(&row.columns[i])).map(|(_, v)| v.clone())
                .or_else(|| (star && map.is_empty()).then(|| row.columns[i].clone()));
            match landed {
                None => dropped.push(format!("{} = {}", row.columns[i], shown(&row.values[i]))),
                Some(landed) => {
                    columns.push(landed);
                    values.push(row.values[i].clone());
                }
            }
        }
        Pending { columns, values, dropped, ..row.clone() }
    }).collect()
}
