//! AN `<ng-template #ref>` HANDED TO A CHILD RENDERS WHERE THE CHILD'S OUTLETS RENDER IT (#146).
//!
//! `key_outlets` follows outlets in the template that declares the ref, and refuses a ref a `@ViewChild` queries: the class
//! holds it then, and it renders wherever the class hands it. One hand-over is followed: the ref queried BY NAME into a
//! field, that field (or one the class assigns it to, `this.slot = this.special`) bound as a bare read to a child
//! component's input, and the child reading that input only as a bare `[ngTemplateOutlet]`/`*ngTemplateOutlet` - or in a
//! condition (`*ngIf="!!slot"`), which renders nothing. Each way in is then the binding element's chain in the parent,
//! the child outlet's chain, and the branches the assignment stands under. Anything else - a second write of the field,
//! another read of it or of the ref, a child that writes its input - keeps the contents' own chain, as before.

use super::key_outlets::{bare_read, text, OUTLET_INPUTS};
use super::store::Store;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// One way a handed template reaches the screen.
pub struct Handed {
    /// The parent's element binding the field to the child's input.
    pub binding: String,
    /// The child's outlet node rendering the input.
    pub outlet: String,
    /// The branches of the assignment that put the template in the field, innermost first; none for the queried field itself.
    pub branches: Vec<String>,
}

/// What the parent class reads to hand a ref on: `(class, ref name) -> the field each by-name query fills`.
pub type Queried = HashMap<(String, String), Vec<String>>;

/// Every outlet-less `ng-template` node a hand-over renders -> its ways. `refs`: `(template, ref name) -> the node`, for the
/// refs no expression of their own template reads.
pub fn find(store: &Store<'_>, refs: &HashMap<(String, String), String>, class_of: &HashMap<String, String>, queried: &Queried) -> HashMap<String, Vec<Handed>> {
    let mut out: HashMap<String, Vec<Handed>> = HashMap::new();
    if refs.is_empty() { return out; }
    let members: Vec<(String, String, String, Option<String>)> = store.table("members").iter()
        .filter_map(|m| Some((text(m, "id")?, text(m, "class")?, text(m, "name")?, text(m, "value"))))
        .collect();
    let parent_of: HashMap<String, String> = store.table("branches").iter()
        .filter_map(|b| Some((text(b, "id")?, text(b, "parent")?))).collect();
    // `member id -> every assignment to it`: `(class, the value's own text when it is a property access, its branch)`.
    let mut writes: HashMap<String, Vec<(Option<String>, Option<String>, Option<String>)>> = HashMap::new();
    for a in store.table("assignments").iter() {
        let Some(target) = text(a, "target_id") else { continue };
        let value = a.get("value");
        let access = value.filter(|v| v.get("$kind").and_then(Value::as_str) == Some("PropertyAccessExpression"))
            .and_then(|v| v.get("$expr")).and_then(Value::as_str).map(str::to_string);
        writes.entry(target).or_default().push((text(a, "class"), access, text(a, "branch")));
    }
    let mut renders: HashMap<String, String> = HashMap::new();
    for r in store.table("renders").iter() {
        if let (Some(node), Some(to)) = (text(r, "node"), text(r, "to")) { renders.insert(node, to); }
    }
    let class_of_component: HashMap<String, String> = store.table("components").iter()
        .filter_map(|c| Some((text(c, "id")?, text(c, "class")?))).collect();
    let template_of: HashMap<String, String> = class_of.iter().map(|(t, c)| (c.clone(), t.clone())).collect();
    // HOW OFTEN EACH FILE READS A PROPERTY OF EACH NAME (`files.prop_reads`), and the files that hand `this` away.
    let mut prop_reads: HashMap<String, HashMap<String, i64>> = HashMap::new();
    let mut indexed: HashSet<String> = HashSet::new();
    for f in store.table("files").iter() {
        let Some(id) = text(f, "id") else { continue };
        if f.get("this_indexed").and_then(Value::as_i64).is_some_and(|n| n > 0) { indexed.insert(id.clone()); }
        if let Some(Value::Object(counts)) = f.get("prop_reads") {
            prop_reads.insert(id, counts.iter().filter_map(|(k, v)| Some((k.clone(), v.as_i64()?))).collect());
        }
    }
    let read_in = |file: &str, name: &str| prop_reads.get(file).and_then(|c| c.get(name)).copied().unwrap_or(0);
    let read_elsewhere = |file: &str, name: &str| prop_reads.iter().any(|(f, c)| f != file && c.get(name).is_some_and(|n| *n > 0));
    let file_of_class: HashMap<String, String> = store.table("classes").iter()
        .filter_map(|c| Some((text(c, "id")?, text(c, "file")?))).collect();
    let gate_at: HashSet<(String, String)> = store.table("gates").iter()
        .filter_map(|g| Some((text(g, "node")?, text(g, "name")?))).collect();
    // Every template expression, by its template: `(node, kind, name, identifiers, is the bare read of each)`.
    let expressions = store.table("expressions");
    let mut reads: HashMap<String, Vec<&super::store::Row>> = HashMap::new();
    for e in expressions.iter() {
        if let Some(t) = text(e, "template") { reads.entry(t).or_default().push(e); }
    }
    let names = |e: &super::store::Row| -> Vec<String> {
        match e.get("identifiers") {
            Some(Value::Array(items)) => items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
            _ => Vec::new(),
        }
    };
    for ((tpl, name), node) in refs {
        let Some(class) = class_of.get(tpl) else { continue };
        let Some([field]) = queried.get(&(class.clone(), name.clone())).map(Vec::as_slice) else { continue };
        let field_id = members.iter().find(|(_, c, n, _)| c == class && n == field).map(|(id, ..)| id.clone());
        // THE QUERIED FIELD IS READ ONLY BY THE ASSIGNMENTS HANDING IT ON: any other read (`createEmbeddedView(this.special)`, another
        // file, `this` handed away) renders the template where the map cannot follow.
        let Some(file) = file_of_class.get(class) else { continue };
        let handing = writes.values().flatten().filter(|(c, access, _)| c.as_ref() == Some(class) && access.as_deref() == Some(format!("this.{field}").as_str())).count() as i64;
        if indexed.contains(file) || read_in(file, field) != handing || read_elsewhere(file, field) { continue; }
        // THE FIELDS HOLDING THE TEMPLATE: the queried one, unconditionally, and each one every write of which is `this.<field>`.
        let mut holders: Vec<(String, Vec<Vec<String>>)> = Vec::new();
        if field_id.as_ref().is_some_and(|id| !writes.contains_key(id)) { holders.push((field.clone(), vec![Vec::new()])); }
        let read = format!("this.{field}");
        for (id, c, n, value) in &members {
            if c != class || n == field { continue; }
            let Some(all) = writes.get(id) else { continue };
            let unset = value.as_deref().is_none_or(|v| v == "null" || v == "undefined");
            if !unset || !all.iter().all(|(wc, access, _)| wc.as_ref() == Some(class) && access.as_deref() == Some(read.as_str())) { continue; }
            let ways = all.iter().map(|(_, _, branch)| {
                let mut chain = Vec::new();
                let mut cur = branch.clone();
                let mut seen = HashSet::new();
                while let Some(b) = cur.filter(|b| seen.insert(b.clone())) { cur = parent_of.get(&b).cloned(); chain.push(b); }
                chain
            }).collect();
            holders.push((n.clone(), ways));
        }
        let mut ways: Vec<Handed> = Vec::new();
        let mut refused = false;
        let own = reads.get(tpl).map(Vec::as_slice).unwrap_or(&[]);
        for (holder, assigned) in &holders {
            for e in own.iter().filter(|e| names(e).contains(holder)) {
                let bound = matches!(text(e, "kind").as_deref(), Some("input")) && bare_read(e.get("ast"), holder);
                let (Some(binding), Some(input)) = (text(e, "node").filter(|_| bound), text(e, "name")) else { refused = true; break };
                let child_tpl = renders.get(&binding).and_then(|k| class_of_component.get(k)).and_then(|c| template_of.get(c));
                let Some(child_tpl) = child_tpl else { refused = true; break };
                // THE CHILD MUST NOT WRITE ITS INPUT, and reads it only to render it or to test it.
                let child_class = &class_of[child_tpl];
                // NOR READS IT IN TYPESCRIPT: `this.slot` handed on there renders it somewhere else.
                let child_file = file_of_class.get(child_class);
                let written = members.iter().any(|(id, c, n, _)| c == child_class && *n == input && writes.contains_key(id))
                    || child_file.is_none_or(|f| indexed.contains(f) || read_in(f, &input) > 0);
                let mut outlets = Vec::new();
                for r in reads.get(child_tpl).map(Vec::as_slice).unwrap_or(&[]).iter().filter(|r| names(r).contains(&input)) {
                    let (Some(at), Some(kind), Some(what)) = (text(r, "node"), text(r, "kind"), text(r, "name")) else { refused = true; break };
                    if matches!(kind.as_str(), "input" | "template_attr") && OUTLET_INPUTS.contains(&what.as_str()) && bare_read(r.get("ast"), &input) {
                        outlets.push(at);
                    } else if !gate_at.contains(&(at, what)) {
                        refused = true;
                        break;
                    }
                }
                if refused || written || outlets.is_empty() { refused = true; break; }
                for outlet in outlets {
                    for branches in assigned {
                        ways.push(Handed { binding: binding.clone(), outlet: outlet.clone(), branches: branches.clone() });
                    }
                }
            }
            if refused { break; }
        }
        if !refused && !ways.is_empty() { out.insert(node.clone(), ways); }
    }
    out
}
