//! THE `if`/`else` A KEY IS ASSIGNED UNDER — the condition a key set in TypeScript needs that
//! no template node states.
//!
//! `tooltip = 'vendor.x.tooltip'` inside `if (this.detail.TierID === TierIDs.X)` binds
//! at ONE template node, and that node's `gate_chain` is all a render site had to offer: the
//! key read as shown wherever the field is, for every vendor. The branch is already a row
//! (`branches`, with its `sense` and its `parent`), and every assignment names the branch it
//! sits in. This joins the two.
//!
//! EACH WRITE IS A WAY IN, NOT A REQUIREMENT. A key written under `A` in one method and under
//! `B` in another renders when EITHER ran, so the site gets one way per write, each joined to
//! its own chain — the (ref, path) rule `keyreach` states for template refs. A write outside
//! any branch, the field's own initializer included, is a way with no condition, and it
//! empties the intersection exactly as it should.
//!
//! THE CHAIN IS EVERY ENCLOSING BRANCH, each with its own polarity: an `else` holds its
//! condition NEGATED, and `else if` needs nothing special because the inner `if` parents to
//! the outer one's `else` row. EVERY ENCLOSING `case` joins it: a statement names the branch
//! and the case it sits in, and each has its own parent chain — see `gate_cases` for what a
//! case restricts.
//!
//! A KEY CHOSEN BY A TERNARY OR A LOGICAL OPERATOR is one more link in that chain: in
//! `tip = c ? 'a' : 'b'` the key `a` holds `c` and `b` holds `!c`, and so on down a nested
//! one. The link is the id of the condition's own `expressions` row, `!`-prefixed for the
//! negated arm, since one tree is read under both polarities. `a && 'k'` gives `k` under
//! `a`, `a || 'k'` under `!a`; `a ?? 'k'` states nothing a gate can read, so `k` gets no
//! link at all. A key anywhere else in a value - a call's argument, an object, a list - is not
//! the value, and is not a write of it. A CONSTANT the evaluator resolved (`KEYS.hint`) is:
//! it stands for its string exactly as the literal would.
//!
//! A branch is handed to the gate readers AS A CONDITION, never as a gate: `gates.id` stays
//! the only thing a `gates` column names, and the branch ids get columns of their own.

use super::gate_features::Conds;
use super::gate_tsrows::TsRows;
use super::gate_values::EnumIndex;
use super::store::{Row, Store};
use indexmap::{IndexMap, IndexSet};
use serde_json::Value;

fn cell(row: &Row, field: &str) -> Option<String> {
    match row.get(field) {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.is_empty() => None,
        Some(Value::String(s)) => Some(s.clone()),
        Some(other) => Some(other.to_string()),
    }
}

fn number(row: &Row, field: &str) -> Option<i64> {
    match row.get(field) {
        Some(Value::Number(n)) => n.as_i64(),
        Some(Value::String(s)) => s.parse().ok(),
        _ => None,
    }
}

/// Every `branches` row by id: its parent, whether it is the `else`, its tree and its member.
#[derive(Debug, Default)]
pub struct Branches {
    by_id: IndexMap<String, (Option<String>, bool, Option<String>, Option<String>)>,
    /// `switch_cases` id -> the case it is nested in.
    cases: IndexMap<String, Option<String>>,
    /// A ternary's or a logical operator's condition row -> the member the value sits in.
    choices: IndexMap<String, Option<String>>,
}

/// THE TRANSLATION KEYS, SORTED as well as hashed, so a template literal reaches only the keys its literal head
/// can spell. `value_keys` tried EVERY key against every template and concatenation in the tree, most of a large
/// closure; a key a template spells starts with its head, and those are one run of the sorted list.
#[derive(Default)]
pub struct KeySet {
    set: std::collections::HashSet<String>,
    sorted: Vec<String>,
}

impl KeySet {
    pub fn contains(&self, key: &str) -> bool {
        self.set.contains(key)
    }

    /// Every key that starts with `head`, in byte order.
    fn starting_with(&self, head: &str) -> &[String] {
        let from = self.sorted.partition_point(|k| k.as_str() < head);
        let to = from + self.sorted[from..].partition_point(|k| k.starts_with(head));
        &self.sorted[from..to]
    }
}

impl FromIterator<String> for KeySet {
    fn from_iter<I: IntoIterator<Item = String>>(iter: I) -> Self {
        let set: std::collections::HashSet<String> = iter.into_iter().collect();
        let mut sorted: Vec<String> = set.iter().cloned().collect();
        sorted.sort();
        KeySet { set, sorted }
    }
}

/// The texts a key's holes hold when `pieces` (a template literal's literal parts) spell it:
/// every piece in order, every hole at least one character. The first way to split it.
fn segments(key: &str, pieces: &[&str]) -> Option<Vec<String>> {
    fn rest(key: &str, pieces: &[&str], out: &mut Vec<String>) -> bool {
        let [piece, more @ ..] = pieces else { return key.is_empty() };
        if more.is_empty() {
            // The last piece closes the key; the hole before it is everything up to it.
            return key.len() > piece.len() && key.ends_with(piece) && {
                out.push(key[..key.len() - piece.len()].to_string());
                true
            };
        }
        let mut from = 1;
        while let Some(at) = key.get(from..).and_then(|k| k.find(piece)).map(|i| i + from) {
            out.push(key[..at].to_string());
            if rest(&key[at + piece.len()..], more, out) {
                return true;
            }
            out.pop();
            from = at + 1;
        }
        false
    }
    let [head, tail @ ..] = pieces else { return None };
    if tail.is_empty() || !key.starts_with(head) {
        return None;
    }
    let mut out = Vec::new();
    rest(&key[head.len()..], tail, &mut out).then_some(out)
}

/// The string a RESOLVED CONSTANT stands for: `KEYS.hint` over `const KEYS = {hint: 'k'}` is
/// `{"$member": "KEYS.hint", "value": "k"}`, and it is `k` exactly as the literal is. Read BY
/// SHAPE - a marked object (a `$` key) carrying a `value` that is a string or another such
/// object - never by a list of the evaluator's marker names. A value that is an array or a
/// plain object is not a string, so the member of a const LIST of rules stays unread.
fn constant(o: &serde_json::Map<String, Value>) -> Option<&str> {
    if !o.keys().any(|k| k.starts_with('$')) {
        return None;
    }
    match o.get("value")? {
        Value::String(s) => Some(s),
        Value::Object(inner) => constant(inner),
        _ => None,
    }
}

/// A `+` chain as a template's pieces: every string (a constant's included) joins the piece it
/// follows, and anything else is a HOLE that opens the next one. Two holes side by side are one,
/// since no text between them can be told apart.
fn concat_pieces(v: &Value, pieces: &mut Vec<String>, holes: &mut usize) {
    let text = match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => n.as_i64().map(|i| i.to_string()),
        Value::Object(o) if o.contains_key("$concat") => {
            for x in o.get("$concat").and_then(|c| c.as_array()).into_iter().flatten() {
                concat_pieces(x, pieces, holes);
            }
            return;
        }
        Value::Object(o) => constant(o).map(str::to_string),
        _ => None,
    };
    match text {
        Some(t) => pieces.last_mut().expect("a first piece").push_str(&t),
        None if *holes > 0 && pieces.last().is_some_and(|p| p.is_empty()) => {}
        None => {
            *holes += 1;
            pieces.push(String::new());
        }
    }
}

/// Every key a value can BE, each with the choice links that pick it, joined onto `chain`.
///
/// A TEMPLATE LITERAL CAN BE EVERY KEY ITS PIECES SPELL. `` `menu.${name}.label` ``
/// is each locale key between those two pieces, and it is written under the same chain as a
/// literal would be. The link `tpl:<tree>=<holes>` records WHICH texts the holes held, so a
/// hole written `Enum[x]` can later say the key needs `x` to be that member. A template whose
/// first piece is empty is read as nothing: it matches keys by their ending alone. A `+` chain
/// (`'regions.' + name`) is read the same way, with no tree to name its holes by.
///
/// A RESOLVED CONSTANT is the string it stands for - see `constant`.
pub fn value_keys(
    v: &Value,
    chain: &[String],
    keys: &KeySet,
    out: &mut Vec<(String, Vec<String>)>,
) {
    let with = |link: String| -> Vec<String> {
        let mut c = chain.to_vec();
        c.push(link);
        c
    };
    match v {
        Value::String(s) if keys.contains(s) => out.push((s.clone(), chain.to_vec())),
        Value::Object(o) if o.contains_key("$template") => {
            let pieces: Vec<&str> = o.get("$template").and_then(|t| t.as_array()).into_iter().flatten()
                .filter_map(|p| p.as_str()).collect();
            if pieces.first().is_none_or(|p| p.is_empty()) {
                return;
            }
            let tree = o.get("$expr_id").and_then(|x| x.as_str());
            for key in keys.starting_with(pieces[0]) {
                let Some(holes) = segments(key, &pieces) else { continue };
                let mut c = chain.to_vec();
                if let Some(tree) = tree {
                    c.push(format!("tpl:{tree}={}", holes.join("|")));
                }
                out.push((key.clone(), c));
            }
        }
        Value::Object(o) if o.contains_key("$concat") => {
            let mut pieces = vec![String::new()];
            let mut holes = 0;
            concat_pieces(v, &mut pieces, &mut holes);
            if holes == 0 && keys.contains(&pieces[0]) {
                // Every operand was known - a constant's included - so the chain IS one string.
                out.push((pieces[0].clone(), chain.to_vec()));
            }
            if holes == 0 || pieces[0].is_empty() {
                return;
            }
            let pieces: Vec<&str> = pieces.iter().map(String::as_str).collect();
            for key in keys.starting_with(pieces[0]) {
                if segments(key, &pieces).is_some() {
                    out.push((key.clone(), chain.to_vec()));
                }
            }
        }
        Value::Object(o) if constant(o).is_some() => {
            let s = constant(o).unwrap_or_default();
            if keys.contains(s) {
                out.push((s.to_string(), chain.to_vec()));
            }
        }
        Value::Object(o) => {
            let Some(Value::String(cond)) = o.get("$cond_expr") else { return };
            if o.contains_key("$then") || o.contains_key("$else") {
                if let Some(t) = o.get("$then") {
                    value_keys(t, &with(cond.clone()), keys, out);
                }
                if let Some(e) = o.get("$else") {
                    value_keys(e, &with(format!("!{cond}")), keys, out);
                }
                return;
            }
            let Some(Value::Array(ops)) = o.get("$operands") else { return };
            let [left, right] = ops.as_slice() else { return };
            match o.get("$logic").and_then(|l| l.as_str()) {
                Some("&&") => value_keys(right, &with(cond.clone()), keys, out),
                Some("||") => {
                    value_keys(left, &with(cond.clone()), keys, out);
                    value_keys(right, &with(format!("!{cond}")), keys, out);
                }
                Some("??") => {
                    value_keys(left, chain, keys, out);
                    value_keys(right, chain, keys, out);
                }
                _ => {}
            }
        }
        _ => {}
    }
}

/// The conditional arms a statement row sits in, innermost last: `x:12` or `!x:12`.
fn choice_links(row: &Row) -> Vec<String> {
    row.get("choices").and_then(|c| c.as_array()).into_iter().flatten()
        .filter_map(|v| v.as_str().filter(|s| !s.is_empty()).map(str::to_string))
        .collect()
}

/// Every choice condition a value holds, down the arms `value_keys` follows.
fn choices_in(v: &Value, member: &Option<String>, out: &mut IndexMap<String, Option<String>>) {
    let Value::Object(o) = v else { return };
    let Some(Value::String(cond)) = o.get("$cond_expr") else { return };
    out.insert(cond.clone(), member.clone());
    for arm in ["$then", "$else"].iter().filter_map(|k| o.get(*k)) {
        choices_in(arm, member, out);
    }
    if let Some(Value::Array(ops)) = o.get("$operands") {
        ops.iter().for_each(|x| choices_in(x, member, out));
    }
}

impl Branches {
    pub fn new(store: &Store<'_>) -> Branches {
        let mut out = Branches::default();
        for b in store.table("branches").iter() {
            let Some(id) = cell(b, "id") else { continue };
            let negated = cell(b, "sense").as_deref() == Some("else");
            out.by_id.insert(id, (cell(b, "parent"), negated, cell(b, "condition_expr"), cell(b, "member")));
        }
        for c in store.table("switch_cases").iter() {
            if let Some(id) = cell(c, "id") {
                out.cases.insert(id, cell(c, "parent"));
            }
        }
        for table in ["assignments", "returns", "members"] {
            for r in store.table(table).iter() {
                let member = if table == "members" { cell(r, "id") } else { cell(r, "member") };
                if let Some(v) = r.get("value") {
                    choices_in(v, &member, &mut out.choices);
                }
            }
        }
        // A CALL'S ARGUMENTS choose keys too: `translate(c ? 'a' : 'b')`.
        for c in store.table("calls").iter() {
            let member = cell(c, "member");
            for arg in c.get("args").and_then(|a| a.as_array()).into_iter().flatten() {
                choices_in(arg, &member, &mut out.choices);
            }
        }
        // AND A STATEMENT SITS IN ONE: `c ? this.t('a') : this.t('b')` stamps each call with the
        // arm it runs in - see `TsBody.mjs`. The link is the condition's row, `!` for the false arm.
        for table in ["calls", "assignments", "returns", "locals"] {
            for r in store.table(table).iter() {
                let member = cell(r, "member");
                for link in choice_links(r) {
                    let id = link.strip_prefix('!').unwrap_or(&link).to_string();
                    out.choices.entry(id).or_insert_with(|| member.clone());
                }
            }
        }
        out
    }

    /// Which side of its condition an id stands for: `then`/`else` for a branch or a choice,
    /// `case` for a switch case.
    pub fn sense(&self, id: &str) -> &'static str {
        if id.starts_with("tpl:") {
            return "built";
        }
        if self.cases.contains_key(id) {
            return "case";
        }
        match self.by_id.get(id) {
            Some((_, true, ..)) => "else",
            Some(_) => "then",
            None if id.starts_with('!') => "else",
            None => "then",
        }
    }

    pub fn contains(&self, id: &str) -> bool {
        let choice = id.strip_prefix('!').unwrap_or(id);
        id.starts_with("tpl:") || self.by_id.contains_key(id) || self.cases.contains_key(id) || self.choices.contains_key(choice)
    }

    /// Every branch, every case and every conditional ARM a statement row sits in.
    pub fn chain_of(&self, row: &Row) -> Vec<String> {
        let mut out = choice_links(row);
        out.extend(self.chain(cell(row, "branch")));
        let mut current = cell(row, "case");
        while let Some(id) = current {
            if out.contains(&id) {
                break;
            }
            let Some(parent) = self.cases.get(&id) else { break };
            out.push(id);
            current = parent.clone();
        }
        out
    }

    /// The branch a statement sits in and every one around it, innermost first. A parent
    /// seen twice ends the walk rather than looping.
    pub fn chain(&self, start: Option<String>) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut current = start;
        while let Some(id) = current {
            if out.contains(&id) {
                break;
            }
            let Some((parent, ..)) = self.by_id.get(&id) else { break };
            out.push(id);
            current = parent.clone();
        }
        out
    }

    /// Every branch that has a tree, as a condition the gate readers can walk.
    ///
    /// The tree is RESOLVED first: its reads carry `{file, line}` anchors, and every reader
    /// asks for `row`. The names the member's callbacks bind are marked, so a `.some((x) =>
    /// x === v)` can tell its element from its value.
    pub fn conds(&self, store: &Store<'_>, idx: &EnumIndex) -> Conds {
        let step = crate::trace::stage("branches: read the rows");
        let rows = TsRows::new(store);
        let mut bound: IndexMap<String, IndexSet<String>> = IndexMap::new();
        for f in store.table("functions").iter() {
            let Some(parent) = cell(f, "parent") else { continue };
            let Some(Value::Array(params)) = f.get("params") else { continue };
            let names = params.iter().filter_map(|p| p.get("name").and_then(|n| n.as_str()).map(str::to_string));
            bound.entry(parent).or_default().extend(names);
        }
        let expressions = store.table("expressions");
        // BY ROW, NOT BY TREE: an AST is decoded when it is read (`store::Cell`).
        let mut rows_of: IndexMap<String, &Row> = IndexMap::new();
        for e in expressions.iter() {
            if let Some(id) = cell(e, "id") {
                rows_of.insert(id, e);
            }
        }
        let ast_of = |id: &str| rows_of.get(id).and_then(|r| r.get("ast")).filter(|a| !a.is_null());
        let none = IndexSet::new();
        let mut out = Conds::new();
        drop(step);
        let step = crate::trace::stage("branches: resolve the conditions");
        for (id, (_, negated, tree, member)) in &self.by_id {
            let Some(ast) = tree.as_ref().and_then(|t| ast_of(t)) else { continue };
            let names = member.as_ref().and_then(|m| bound.get(m)).unwrap_or(&none);
            let resolved = rows.resolve_in(super::astreads::unwrap(ast), names, idx, member.as_deref());
            out.insert(id.clone(), (resolved, *negated));
        }
        drop(step);
        let _step = crate::trace::stage("branches: resolve the arms");
        for (id, member) in &self.choices {
            let Some(ast) = ast_of(id) else { continue };
            let names = member.as_ref().and_then(|m| bound.get(m)).unwrap_or(&none);
            let resolved = rows.resolve_in(super::astreads::unwrap(ast), names, idx, member.as_deref());
            out.insert(format!("!{id}"), (resolved.clone(), true));
            out.insert(id.clone(), (resolved, false));
        }
        out
    }
}

/// The distinct chains, in the order first seen.
fn push_way(ways: &mut Vec<Vec<String>>, chain: Vec<String>) {
    if !ways.contains(&chain) {
        ways.push(chain);
    }
}

/// `key + " " + file` -> one way per place a `.ts` literal of that key sits: the chain of
/// the innermost statement that holds it AS A VALUE - an assignment, a `return`, or a call's
/// argument - or no condition when none does. A key returned or passed under a branch is
/// exactly as conditional as one assigned under it.
///
/// A LITERAL NO ASSIGNMENT ENCLOSES IS A WAY WITH NO CONDITION. It may be a return, an
/// argument, an initializer — somewhere this pass does not follow — and calling it guarded
/// by the branch a sibling literal sits in would claim a condition that way does not have.
pub fn literal_ways(store: &Store<'_>, branches: &Branches) -> IndexMap<String, Vec<Vec<String>>> {
    let keys: KeySet =
        store.table("translations").iter().filter_map(|t| cell(t, "key")).collect();
    let mut writes: IndexMap<String, Vec<(i64, i64, Vec<String>)>> = IndexMap::new();
    let (assignments, returns, calls) = (store.table("assignments"), store.table("returns"), store.table("calls"));
    let statements = assignments.iter().chain(returns.iter()).map(|r| (r, r.get("value").into_iter().collect::<Vec<_>>()))
        .chain(calls.iter().map(|c| (c, c.get("args").and_then(|a| a.as_array()).into_iter().flatten().collect())));
    for (row, values) in statements {
        let (Some(file), Some(line), Some(end)) = (cell(row, "owner_file"), number(row, "line"), number(row, "end_line")) else { continue };
        let mut found = Vec::new();
        let chain = branches.chain_of(row);
        for v in values {
            value_keys(v, &chain, &keys, &mut found);
        }
        for (key, chain) in found {
            writes.entry(format!("{key} {file}")).or_default().push((line, end, chain));
        }
    }
    let mut out: IndexMap<String, Vec<Vec<String>>> = IndexMap::new();
    for s in store.table("string_literals").iter() {
        let Some(Value::String(value)) = s.get("value") else { continue };
        let (Some(file), Some(line)) = (cell(s, "file"), number(s, "line")) else { continue };
        if !keys.contains(value) {
            continue;
        }
        let k = format!("{value} {file}");
        // THE INNERMOST ENCLOSING WRITE, as `key_literals` picks its span - and every way that
        // one write gives the key, since `c ? k : k` is two.
        let around: Vec<&(i64, i64, Vec<String>)> = writes
            .get(&k)
            .map(|list| list.iter().filter(|(l, e, _)| *l <= line && line <= *e).collect())
            .unwrap_or_default();
        let tightest = around.iter().map(|(l, e, _)| e - l).min();
        let ways = out.entry(k).or_default();
        match tightest {
            None => push_way(ways, Vec::new()),
            Some(span) => around
                .iter()
                .filter(|(l, e, _)| e - l == span)
                .for_each(|(_, _, c)| push_way(ways, c.clone())),
        }
    }
    // A KEY ONLY A TEMPLATE BUILDS has no literal to anchor it: every statement that builds it
    // is a way in, under that statement's chain.
    for (k, list) in writes {
        if out.contains_key(&k) {
            continue;
        }
        let ways = out.entry(k).or_default();
        for (_, _, c) in list {
            push_way(ways, c);
        }
    }
    out
}

/// The ways a member's writes give each key it holds: its initializer is a way with no
/// condition, and every in-class write is a way under that write's own chain.
pub fn member_ways(chains: &mut IndexMap<String, Vec<Vec<String>>>, key: &str, chain: Vec<String>) {
    push_way(chains.entry(key.to_string()).or_default(), chain);
}

#[cfg(test)]
#[path = "key_branches_tests.rs"]
mod tests;
