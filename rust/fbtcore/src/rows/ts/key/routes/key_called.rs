//! THE CONDITIONS A COMPONENT IS CREATED IN CODE UNDER (#56, divergence #45) - the `if` around the creating call,
//! and the `if` around every call of the private method that holds it.
//!
//! `go() { if (kind === Kind.Home) { this.openModal(); } }` over `private openModal() { this.factory.create(Modal, ..) }`
//! creates `Modal` only when the `if` holds, and no template node says so: every key in `Modal` read as shown for every
//! kind. A dynamic render names its creating call (`renders.call`); that call's own chain is the edge's, and so is
//! what EVERY call of its member holds, lifted while the member is one whose callers the map holds all of:
//!
//! - `private` (nothing outside the class can call it, nor implement an interface with it), no decorator, not a
//!   lifecycle hook Angular calls by name;
//! - nothing reads it but its own file's property reads (`MemberReads::own_reads`: no template, no string, no other
//!   file, `this` never handed away), and every one of those reads is a `calls` row resolved to it - so
//!   `setTimeout(this.open)` keeps the member unread.
//!
//! The edge holds the INTERSECTION over the callers, each caller's chain joined to what its own member lifts, as
//! `key_returned` folds the ways a factory returned a class. A member called from itself lifts nothing.

use super::key_branches::Branches;
use super::key_unread::{text, MemberReads};
use super::store::{Row, Store};
use indexmap::{IndexMap, IndexSet};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

const HOOKS: [&str; 8] = ["ngOnChanges", "ngOnInit", "ngDoCheck", "ngAfterContentInit", "ngAfterContentChecked",
    "ngAfterViewInit", "ngAfterViewChecked", "ngOnDestroy"];

/// WHAT EVERY CALL OF A MEMBER HOLDS, for each member whose callers the map holds all of - see the module. Built once
/// per pass; a member is folded once and remembered.
pub struct CallLift {
    reads: MemberReads,
    calls: Rc<Vec<Row>>,
    members: Rc<Vec<Row>>,
    member_at: HashMap<String, usize>,
    callers: HashMap<String, Vec<usize>>,
    done: RefCell<HashMap<String, Option<IndexSet<String>>>>,
}

impl CallLift {
    pub fn new(store: &Store<'_>) -> CallLift {
        let calls = store.table("calls");
        let members = store.table("members");
        let mut callers: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, c) in calls.iter().enumerate() {
            if let Some(target) = text(c, "target_id") {
                callers.entry(target).or_default().push(i);
            }
        }
        let member_at = members.iter().enumerate().filter_map(|(i, m)| Some((text(m, "id")?, i))).collect();
        CallLift { reads: MemberReads::new(store), calls, members, member_at, callers, done: RefCell::new(HashMap::new()) }
    }

    /// The chain a statement of `member` runs under through its callers; empty when nothing is proven.
    pub fn of(&self, member: &str, branches: &Branches) -> Vec<String> {
        self.lift(member, branches, &mut HashSet::new()).map(|s| s.into_iter().collect()).unwrap_or_default()
    }

    /// `row`'s own chain, then what its member lifts.
    pub fn chain_of(&self, row: &Row, branches: &Branches) -> Vec<String> {
        let mut out = branches.chain_of(row);
        if let Some(member) = text(row, "member") {
            for b in self.of(&member, branches) {
                if !out.contains(&b) {
                    out.push(b);
                }
            }
        }
        out
    }

    fn lift(&self, member: &str, branches: &Branches, walking: &mut HashSet<String>) -> Option<IndexSet<String>> {
        if let Some(known) = self.done.borrow().get(member) {
            return known.clone();
        }
        if !walking.insert(member.to_string()) {
            return None;
        }
        let out = self.compute(member, branches, walking);
        walking.remove(member);
        self.done.borrow_mut().insert(member.to_string(), out.clone());
        out
    }

    fn compute(&self, member: &str, branches: &Branches, walking: &mut HashSet<String>) -> Option<IndexSet<String>> {
        let m = &self.members[*self.member_at.get(member)?];
        let name = text(m, "name")?;
        if text(m, "visibility").as_deref() != Some("private") || HOOKS.contains(&name.as_str()) {
            return None;
        }
        let callers: Vec<&Row> = self.callers.get(member).into_iter().flatten().map(|i| &self.calls[*i]).collect();
        let own = self.reads.own_reads(m)?;
        let file = text(m, "file");
        if callers.is_empty() || own != callers.len() as i64 || callers.iter().any(|c| text(c, "file") != file) {
            return None;
        }
        let mut held: Option<IndexSet<String>> = None;
        for call in callers {
            let mut way: IndexSet<String> = branches.chain_of(call).into_iter().collect();
            if let Some(from) = text(call, "member") {
                // A CALLER THE MAP CANNOT LIFT still holds its own chain: the call runs only under it.
                way.extend(self.lift(&from, branches, walking).unwrap_or_default());
                if walking.contains(&from) {
                    return None;
                }
            }
            held = Some(match held {
                None => way,
                Some(h) => h.into_iter().filter(|b| way.contains(b)).collect(),
            });
        }
        held
    }
}

/// `renders.id` -> what the creating call and every call of its member hold. A render with no `call`, or holding
/// nothing, has no entry.
pub fn called_under(store: &Store<'_>, branches: &Branches) -> IndexMap<String, Vec<String>> {
    let mut out = IndexMap::new();
    let lift = CallLift::new(store);
    let call_by_id: HashMap<String, usize> = lift.calls.iter().enumerate().filter_map(|(i, c)| Some((text(c, "id")?, i))).collect();
    for r in store.table("renders").iter() {
        let (Some(id), Some(at)) = (text(r, "id"), text(r, "call").and_then(|c| call_by_id.get(&c).copied())) else { continue };
        let held = lift.chain_of(&lift.calls[at], branches);
        if !held.is_empty() {
            out.insert(id, held);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn store(open_reads: i64, visibility: &str) -> Store<'static> {
        let tables = json!({
            "files": [{"id": "f:1", "prop_reads": {"open": open_reads}}],
            "branches": [{"id": "br:1", "sense": "then"}, {"id": "br:2", "sense": "then"}],
            "members": [
                {"id": "m:1", "name": "go", "file": "f:1", "visibility": "public", "kind": "method"},
                {"id": "m:2", "name": "open", "file": "f:1", "visibility": visibility, "kind": "method"},
            ],
            "calls": [
                {"id": "cl:1", "member": "m:1", "file": "f:1", "target_id": "m:2", "branch": "br:1"},
                {"id": "cl:2", "member": "m:2", "file": "f:1", "branch": "br:2"},
            ],
            "renders": [{"id": "rd:1", "call": "cl:2"}],
        });
        Store::from_payload(tables.as_object().cloned().unwrap(), "typescript")
    }

    #[test]
    fn a_private_method_called_under_a_branch_lifts_it_onto_the_edge() {
        let s = store(1, "private");
        let out = called_under(&s, &Branches::new(&s));
        assert_eq!(out["rd:1"], vec!["br:2".to_string(), "br:1".to_string()]);
    }

    #[test]
    fn a_public_method_or_a_read_that_is_no_call_lifts_nothing() {
        for s in [store(1, "public"), store(2, "private")] {
            let out = called_under(&s, &Branches::new(&s));
            assert_eq!(out["rd:1"], vec!["br:2".to_string()]);
        }
    }
}
