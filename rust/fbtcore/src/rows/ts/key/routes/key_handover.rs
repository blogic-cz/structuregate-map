//! AN `<ng-template #ref>` HANDED TO A CHILD RENDERS WHERE THE CHILD'S OUTLETS RENDER IT (#146).
//!
//! `key_outlets` follows outlets in the template that declares the ref, and refuses a ref a `@ViewChild` queries: the class
//! holds it then, and it renders wherever the class hands it. One hand-over is followed: the ref queried BY NAME into a
//! field, that field (or one the class assigns it to, `this.slot = this.special`) bound as a bare read to a child
//! component's input, and the child reading that input only as a bare `[ngTemplateOutlet]`/`*ngTemplateOutlet` - or in a
//! condition (`*ngIf="!!slot"`), which renders nothing - or RELAYING it as a bare read to a component of its own, which is followed
//! the same way. Each way in is then the binding element's chain in the parent, each relaying element's in its child, the last
//! child outlet's chain, and the branches the assignment stands under. Anything else - a second write of the field, another read
//! of it or of the ref, a child that writes its input, one holder the map cannot follow - keeps the contents' own chain, as before.

use super::key_outlets::{bare_read, text, OUTLET_INPUTS};
use super::store::{Row, Store};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// One way a handed template reaches the screen.
pub struct Handed {
    /// The parent's element binding the field to the child's input, then each child's element relaying it on to its own child.
    pub bindings: Vec<String>,
    /// The last child's outlet node rendering the input.
    pub outlet: String,
    /// What the assignment that put the template in the field stands under (`Branches::chain_of`: branches, cases, arms); none for
    /// the queried field itself.
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
    // WHAT AN ASSIGNMENT STANDS UNDER, as a key's own write does (#4, `Branches::chain_of`): its `if`s, its `case`s, an arm.
    let branches = store.shared("branches", super::key_branches::Branches::new);
    // EVERY TEMPLATE HANDLER CALLING A METHOD (`(click)="apply(kind)"`, the method by its `target.row`): its call hands the method's parameters.
    let mut handlers: HashMap<String, Vec<String>> = HashMap::new();
    for e in store.table("expressions").iter() {
        let (Some(id), Some(ast)) = (text(e, "id"), e.get("ast")) else { continue };
        let call = super::astreads::unwrap(ast);
        if call.get("k").and_then(Value::as_str) != Some("Call") { continue; }
        if let Some(method) = call.get("receiver").and_then(|r| r.get("target")).and_then(|t| t.get("row")).and_then(Value::as_str) {
            handlers.entry(method.to_string()).or_default().push(id);
        }
    }
    // THE METHODS THAT TAKE A PARAMETER: only their branches can test what a caller hands.
    let takes: HashSet<String> = store.table("members").iter()
        .filter(|m| m.get("params").and_then(Value::as_array).is_some_and(|p| !p.is_empty()))
        .filter_map(|m| text(m, "id")).collect();
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
    // EVERY STATEMENT CALL RESOLVED TO A METHOD TAKING PARAMETERS, and how many resolved calls of each name each file holds.
    let mut stated: HashMap<String, Vec<String>> = HashMap::new();
    let mut resolved: HashMap<(String, String), i64> = HashMap::new();
    for c in store.table("calls").iter() {
        let Some(target) = text(c, "target_id") else { continue };
        if let (Some(file), Some(method)) = (text(c, "file"), text(c, "method")) { *resolved.entry((file, method)).or_default() += 1; }
        if takes.contains(&target) && let Some(id) = text(c, "id") { stated.entry(target).or_default().push(id); }
    }
    // A METHOD READ BY MORE THAN ITS RESOLVED CALLS (`setTimeout(this.apply)`, an unresolved `x.apply(..)`, `this` handed away) may be
    // called with anything: one of its ways keeps the test as written.
    let opaque: HashSet<String> = members.iter().filter(|(id, ..)| takes.contains(id))
        .filter(|(_, class, name, _)| file_of_class.get(class).is_none_or(|f| indexed.contains(f))
            || prop_reads.iter().any(|(f, counts)| counts.get(name).is_some_and(|n| *n > resolved.get(&(f.clone(), name.clone())).copied().unwrap_or(0))))
        .map(|(id, ..)| id.clone()).collect();
    let callers = Callers { branches: &branches, handlers: &handlers, stated: &stated, takes: &takes, opaque: &opaque };
    // `member id -> every assignment to it`: `(class, the value's own text when it is a property access, what it stands under)`.
    let mut writes: HashMap<String, Vec<(Option<String>, Option<String>, Vec<Vec<String>>)>> = HashMap::new();
    for a in store.table("assignments").iter() {
        let Some(target) = text(a, "target_id") else { continue };
        let value = a.get("value");
        let access = value.filter(|v| v.get("$kind").and_then(Value::as_str) == Some("PropertyAccessExpression"))
            .and_then(|v| v.get("$expr")).and_then(Value::as_str).map(str::to_string);
        writes.entry(target).or_default().push((text(a, "class"), access, callers.ways(branches.chain_of(a), text(a, "member"))));
    }
    let mut renders: HashMap<String, String> = HashMap::new();
    for r in store.table("renders").iter() {
        if let (Some(node), Some(to)) = (text(r, "node"), text(r, "to")) { renders.insert(node, to); }
    }
    let class_of_component: HashMap<String, String> = store.table("components").iter()
        .filter_map(|c| Some((text(c, "id")?, text(c, "class")?))).collect();
    let template_of: HashMap<String, String> = class_of.iter().map(|(t, c)| (c.clone(), t.clone())).collect();
    let gate_at: HashSet<(String, String)> = store.table("gates").iter()
        .filter_map(|g| Some((text(g, "node")?, text(g, "name")?))).collect();
    // Every template expression, by its template: `(node, kind, name, identifiers, is the bare read of each)`.
    let expressions = store.table("expressions");
    let mut reads: HashMap<String, Vec<&Row>> = HashMap::new();
    for e in expressions.iter() {
        if let Some(t) = text(e, "template") { reads.entry(t).or_default().push(e); }
    }
    let at = Child { renders: &renders, class_of_component: &class_of_component, template_of: &template_of, class_of, file_of_class: &file_of_class,
        members: &members, written: &|id: &str| writes.contains_key(id), indexed: &indexed, read_in: &read_in, reads: &reads, gate_at: &gate_at };
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
            let ways = all.iter().flat_map(|(_, _, chains)| chains.iter().cloned()).collect();
            holders.push((n.clone(), ways));
        }
        let mut ways: Vec<Handed> = Vec::new();
        let mut refused = false;
        let own = at.reads.get(tpl).map(Vec::as_slice).unwrap_or(&[]);
        for (holder, assigned) in &holders {
            for e in own.iter().filter(|e| names(e).contains(holder)) {
                let bound = matches!(text(e, "kind").as_deref(), Some("input")) && bare_read(e.get("ast"), holder);
                let (Some(binding), Some(input)) = (text(e, "node").filter(|_| bound), text(e, "name")) else { refused = true; break };
                let Some(outlets) = follow(&binding, &input, &mut HashSet::new(), &at) else { refused = true; break };
                for (bindings, outlet) in outlets {
                    for branches in assigned {
                        ways.push(Handed { bindings: bindings.clone(), outlet: outlet.clone(), branches: branches.clone() });
                    }
                }
            }
            if refused { break; }
        }
        if !refused && !ways.is_empty() { out.insert(node.clone(), ways); }
    }
    out
}

/// What `follow` reads of the tree: the render edges, the classes, their writes and reads, the gates.
struct Child<'a> {
    renders: &'a HashMap<String, String>,
    class_of_component: &'a HashMap<String, String>,
    template_of: &'a HashMap<String, String>,
    class_of: &'a HashMap<String, String>,
    file_of_class: &'a HashMap<String, String>,
    members: &'a [(String, String, String, Option<String>)],
    written: &'a dyn Fn(&str) -> bool,
    indexed: &'a HashSet<String>,
    read_in: &'a dyn Fn(&str, &str) -> i64,
    reads: &'a HashMap<String, Vec<&'a Row>>,
    gate_at: &'a HashSet<(String, String)>,
}

fn names(e: &Row) -> Vec<String> {
    match e.get("identifiers") {
        Some(Value::Array(items)) => items.iter().filter_map(|v| v.as_str().map(str::to_string)).collect(),
        _ => Vec::new(),
    }
}

// WHERE A CHILD RENDERS THE INPUT `binding` HANDS IT: `(bindings, outlet)` per outlet, `bindings` from `binding` on through every
// child relaying the input as a bare read to a component of its own (#146). None: some read of it the map cannot follow.
fn follow(binding: &str, input: &str, seen: &mut HashSet<(String, String)>, at: &Child<'_>) -> Option<Vec<(Vec<String>, String)>> {
    let child_tpl = at.renders.get(binding).and_then(|k| at.class_of_component.get(k)).and_then(|c| at.template_of.get(c))?;
    // A RELAY BACK INTO A TEMPLATE IT CAME THROUGH renders under no outlet of its own.
    if !seen.insert((child_tpl.clone(), input.to_string())) { return None; }
    // THE CHILD MUST NOT WRITE ITS INPUT, NOR READ IT IN TYPESCRIPT: `this.slot` handed on there renders it somewhere else.
    let child_class = &at.class_of[child_tpl];
    let child_file = at.file_of_class.get(child_class);
    let written = at.members.iter().any(|(id, c, n, _)| c == child_class && n == input && (at.written)(id))
        || child_file.is_none_or(|f| at.indexed.contains(f) || (at.read_in)(f, input) > 0);
    if written { return None; }
    let mut out = Vec::new();
    // IT READS IT ONLY TO RENDER IT, TO TEST IT, OR TO HAND IT ON.
    for r in at.reads.get(child_tpl).map(Vec::as_slice).unwrap_or(&[]).iter().filter(|r| names(r).iter().any(|n| n == input)) {
        let (node, kind, what) = (text(r, "node")?, text(r, "kind")?, text(r, "name")?);
        let bare = bare_read(r.get("ast"), input);
        if matches!(kind.as_str(), "input" | "template_attr") && OUTLET_INPUTS.contains(&what.as_str()) && bare {
            out.push((vec![binding.to_string()], node));
        } else if kind == "input" && bare && at.renders.contains_key(&node) {
            for (mut on, outlet) in follow(&node, &what, seen, at)? {
                on.insert(0, binding.to_string());
                out.push((on, outlet));
            }
        } else if !at.gate_at.contains(&(node, what)) {
            return None;
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

/// Who calls the methods an assignment may stand in.
struct Callers<'a> {
    branches: &'a super::key_branches::Branches,
    /// Method -> each template handler calling it.
    handlers: &'a HashMap<String, Vec<String>>,
    /// Method -> each statement call resolved to it.
    stated: &'a HashMap<String, Vec<String>>,
    /// The methods that take a parameter.
    takes: &'a HashSet<String>,
    /// The methods read where no resolved call accounts for it.
    opaque: &'a HashSet<String>,
}

impl Callers<'_> {
    /// WHAT AN ASSIGNMENT STANDS UNDER, ONE LIST PER CALLER (#146, #124): a branch testing the assigning method's own parameter (`if
    /// (isSpecial(kind))` in `apply(kind)`) says nothing until a caller names the value - so each caller is a way of its own, the
    /// branch read as `arg:<branch>@<site>` (`key_args`: the caller's argument in place of the parameter): a template handler
    /// calling the method, or a statement call (`!<calls id>`, its `this.` chains rebuilt as trees). A method nothing calls keeps
    /// the branch as it is; one read where no resolved call accounts for it keeps it too, as one way more.
    fn ways(&self, chain: Vec<String>, member: Option<String>) -> Vec<Vec<String>> {
        let member = member.unwrap_or_default();
        // A METHOD WITH PARAMETERS ONLY: a test in one without is of fields, which every way reads the same.
        let own = |b: &String| self.takes.contains(&member) && self.branches.condition(b).is_some_and(|(_, _, m)| m == Some(member.as_str()));
        let sites: Vec<String> = self.handlers.get(&member).into_iter().flatten().cloned()
            .chain(self.stated.get(&member).into_iter().flatten().map(|c| format!("!{c}"))).collect();
        if sites.is_empty() || !chain.iter().any(own) { return vec![chain]; }
        let mut ways: Vec<Vec<String>> = sites.iter()
            .map(|site| chain.iter().map(|b| if own(b) { format!("arg:{b}@{site}") } else { b.clone() }).collect())
            .collect();
        if self.opaque.contains(&member) { ways.push(chain); }
        ways
    }
}
