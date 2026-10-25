//! WHAT A CALLED METHOD OR FUNCTION HANDS BACK, read for the keys it holds - see `key_fields::callee_keys`.

use super::key_branches::value_keys;
use super::store::{Row, Store};
use serde_json::Value;

/// What `callee_keys` reads a callee's return with.
pub(super) struct Returned<'r, 'a> {
    pub(super) rows: &'r super::gate_tsrows::TsRows,
    pub(super) one_return: &'r std::collections::HashMap<String, &'a Row>,
    pub(super) every: &'r std::collections::HashMap<String, Vec<&'a Row>>,
    pub(super) keys: &'r super::key_branches::KeySet,
    pub(super) i18n: &'r [String],
    /// The config's `i18nDictionaryCalls`: methods that hand back every key under their first argument's prefix.
    pub(super) dict: &'r [String],
    /// Member or `functions` row id -> its parameters' names.
    pub(super) params: &'r std::collections::HashMap<String, Vec<String>>,
    /// `locals` row id -> the `functions` row of the arrow it holds.
    pub(super) fn_of_local: &'r std::collections::HashMap<String, String>,
    /// (member id, local name) -> every `=` written to that local, in source order.
    pub(super) writes: &'r std::collections::HashMap<(String, String), Vec<&'a Row>>,
    /// `functions` row id -> the member or function it sits in.
    pub(super) parent: &'r std::collections::HashMap<String, String>,
    /// Every `.push(..)` of a list, by the local or field it is made on.
    pub(super) pushes: &'r super::key_pushes::Pushes<'a>,
    /// The classes a receiver's declared type stands for - `key_classes`.
    pub(super) classes: &'r super::key_classes::Classes,
    /// The branches a callee's returns sit in, carried through a call a field holds (`key_args`).
    pub(super) branches: &'r super::key_branches::Branches,
}

/// The arguments of a call of a configured `i18nCalls` method, None for any other value.
pub(super) fn translated<'v>(v: &'v Value, i18n: &[String]) -> Option<&'v [Value]> {
    let method = v.get("$target").and_then(|t| t.get("name")).and_then(Value::as_str)?;
    i18n.iter().any(|c| c == method).then(|| v.get("$args").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default())
}

/// THE KEYS UNDER A PREFIX, as the template value `value_keys` already matches: a call of a configured
/// `i18nDictionaryCalls` method whose first argument spells a string reads as `` `<prefix>.${rest}` `` - a hole of
/// one or more characters, dots included, so every key at any depth under the prefix. No `$expr_id`, so no `tpl:`
/// link. A first argument that spells nothing (a variable) is no prefix and reads as nothing.
pub(super) fn dictionary(v: &Value, dict: &[String]) -> Option<Value> {
    let method = v.get("$target").and_then(|t| t.get("name")).and_then(Value::as_str)?;
    if !dict.iter().any(|c| c == method) {
        return None;
    }
    let prefix = spell(v.get("$args").and_then(Value::as_array)?.first()?)?;
    Some(serde_json::json!({"$template": [format!("{prefix}."), ""], "$holes": ["_"]}))
}

/// THE KEYS OF ONE ARGUMENT OF A TRANSLATE CALL: what the value spells, or - an element of a `const` object of strings
/// read by an index no one knows (`KEYS[this.kind]`, `$indexed`) - every value of that object. Which element is read
/// is the run's choice, so no condition is carried, and each value is a key only if the locale holds it.
pub(super) fn arg_keys(arg: &Value, chain: &[String], keys: &super::key_branches::KeySet, out: &mut Vec<(String, Vec<String>)>) {
    match arg.get("$indexed").and_then(Value::as_array) {
        Some(values) => values.iter().for_each(|v| value_keys(v, chain, keys, out)),
        None => value_keys(arg, chain, keys, out),
    }
}

/// An object literal: no key of it is a row marker (`$call`, `$template`, `$cond_expr` ...).
fn plain_object(v: &Value) -> bool {
    v.as_object().is_some_and(|o| o.keys().all(|k| !k.starts_with('$')))
}

/// The `String` methods that hand back their receiver's text, reworked: the call's `$target` is the compiler's own lib.
fn text_method(v: &Value) -> bool {
    const KEEPS: [&str; 10] = ["replace", "replaceAll", "trim", "trimStart", "trimEnd", "toUpperCase", "toLowerCase", "concat", "slice", "substring"];
    let Some(target) = v.get("$target") else { return false };
    let file = target.get("file").and_then(Value::as_str).unwrap_or_default();
    let name = target.get("name").and_then(Value::as_str).unwrap_or_default();
    KEEPS.contains(&name) && file.split(['/', '\\']).last().is_some_and(|f| f.starts_with("lib."))
}

/// `Promise.resolve(x)` of the compiler's own lib (`$target` names `resolve` in `typescript/lib/lib.*.d.ts`), or rxjs's
/// `of(x)` (`of` declared under `rxjs`): a value that hands its argument on. Compared by path pieces.
fn hands_on(v: &Value) -> bool {
    let Some(target) = v.get("$target") else { return false };
    let file = target.get("file").and_then(Value::as_str).unwrap_or_default();
    let pieces: Vec<&str> = file.split(['/', '\\']).collect();
    match target.get("name").and_then(Value::as_str) {
        Some("resolve") => pieces.last().is_some_and(|f| f.starts_with("lib.")) && pieces.windows(2).any(|w| w == ["typescript", "lib"]),
        Some("of") => pieces.contains(&"rxjs"),
        _ => false,
    }
}

/// The text a value spells: a string, or a STRING ENUM's member (`{"$enum": "Codes.A", "value": "A"}`, any marked
/// object carrying a string `value`) as its VALUE. A numeric member, a variable or a property spells nothing.
pub(super) fn spell(v: &Value) -> Option<&str> {
    match v {
        Value::String(s) => Some(s),
        Value::Object(o) if o.keys().any(|k| k.starts_with('$')) => o.get("value").and_then(Value::as_str),
        _ => None,
    }
}

/// Every string a ternary (nested too) of string arms is, None for any other value.
fn arms(v: &Value) -> Option<Vec<Value>> {
    if spell(v).is_some() {
        return Some(vec![v.clone()]);
    }
    let (then, other) = (v.get("$then")?, v.get("$else")?);
    v.get("$cond_expr")?;
    let mut all = arms(then)?;
    all.extend(arms(other)?);
    Some(all)
}

/// The argument lists a call stands for: the product of each ternary-of-strings argument's arms - EVERY one of
/// them, however many - or the list itself when none is one.
pub(super) fn spellings(args: &[Value]) -> Vec<Vec<Value>> {
    let mut lists = vec![Vec::new()];
    for arg in args {
        let options = arms(arg).filter(|_| spell(arg).is_none()).unwrap_or_else(|| vec![arg.clone()]);
        lists = lists.into_iter().flat_map(|l| options.iter().map(move |o| [l.clone(), vec![o.clone()]].concat())).collect();
    }
    lists
}

/// The member a call entered and the arguments it handed over.
struct Frame<'v> {
    member: String,
    args: &'v [Value],
    /// The value a getter returns or a field holds, and the expression row it is written at: a CALL that value is
    /// itself is placed at that row (`call_at`).
    site: Option<(&'v Value, &'v str)>,
    /// The expression row of the return (or local write) being followed: a call INSIDE it is placed as `^<at>` (#48),
    /// found again there by the member it enters (`key_args`). None: the call has no place, so no callee condition rides.
    at: Option<String>,
}

/// HOW BIG AN ENTRY'S ARGUMENTS ARE, in tree nodes. A member still being read is entered again (a recursion handing
/// on other arguments, which may spell other keys) only with arguments NO LARGER than the ones it is being read with,
/// and never with the same ones. Every argument is built out of the source's own subtrees, so only finitely many
/// fit under a size: the recursion ends without a count, and one whose arguments GROW (`f(n + 'a')`) is cut where
/// they would grow - its later ways are not all read, so none of them is claimed (`cyclic`).
fn size(args: &[Value]) -> usize {
    let mut stack: Vec<&Value> = args.iter().collect();
    let mut n = 0;
    while let Some(v) = stack.pop() {
        n += 1;
        match v {
            Value::Array(items) => stack.extend(items),
            Value::Object(map) => stack.extend(map.values()),
            _ => {}
        }
    }
    n
}

/// WHAT ONE READ HAS ENTERED (#48). Each (member, arguments) is read ONCE, its keys kept with its callees' branch links
/// RELATIVE to its own entry (`arg:<branch>@` for its own branch, `arg:<branch>@^E1~^E2` for one below), and every call
/// of it replays them with its own place prepended (`compose`) - linear in the calls, with no bound on how deep they go.
/// A MEMBER REACHED AGAIN WHILE IT IS STILL BEING READ is in a cycle: it, and every member read between, carries no
/// branch link of its own or below - a recursion's ways are not all read, so none of them is claimed.
#[derive(Default)]
pub(super) struct Seen {
    done: std::collections::HashMap<String, Found>,
    /// The members being read, outermost first, each with its (member, arguments) entry and the arguments' `size`.
    active: Vec<(String, String, usize)>,
    cyclic: std::collections::HashSet<String>,
}

type Found = Vec<(String, Vec<String>)>;

/// `found` read through a call placed at `hop` (`E` the call is that expression, `^E` it sits inside it): every
/// relative `arg:` link gains the hop in front. A call with no place keeps no `arg:` link - its arguments are unknown.
fn compose(found: &Found, hop: Option<&str>) -> Found {
    let place = |link: &String| -> Option<String> {
        let Some(rest) = link.strip_prefix("arg:") else { return Some(link.clone()) };
        let (branch, below) = rest.split_once('@')?;
        let hop = hop?;
        Some(if below.is_empty() { format!("arg:{branch}@{hop}") } else { format!("arg:{branch}@{hop}~{below}") })
    };
    found.iter().map(|(key, chain)| (key.clone(), chain.iter().filter_map(place).collect())).collect()
}

impl Returned<'_, '_> {
    /// The member - or, failing that, the free function - a call's `$target` names, entered with the call's
    /// arguments. A FREE FUNCTION IS ENTERED LIKE A METHOD: `this.c = label('a')` over `export const label = (n) =>
    /// `demo.${n}`` hands the field `demo.a`, and reading only class members left every key such a function builds
    /// a `literal` no component reached. Each (callee, arguments) is read once (`Seen`).
    /// Placed at `site` (`E`, or `^E` inside it), the callee's own `if` branches are carried as `arg:<branch>@<site>` - and
    /// a deeper callee's as `arg:<branch>@<site>~^E1~..`, one place per call between (#48) - each condition being the
    /// branch's with every hop's arguments put in place of the parameters below it (`key_args`).
    pub(super) fn call_at(&self, value: Option<&Value>, site: Option<&str>, seen: &mut Seen, out: &mut Found) {
        let Some(value) = value.map(|v| v.get("$await").unwrap_or(v)) else { return };
        let Some(target) = value.get("$target") else { return };
        // A CALL THROUGH A RECEIVER TYPED BY A BASE CLASS OR AN INTERFACE runs the member of every subtype a `new` builds.
        let answering = self.classes.through(value);
        if !answering.is_empty() {
            for (_, member) in answering {
                self.enter(member, value, seen, out, site);
            }
            return;
        }
        let Some(member) = self.rows.member_named(target).or_else(|| self.rows.function_named(target)) else { return };
        self.enter(member, value, seen, out, site);
    }

    /// The keys a value holds through the calls in it, read inside `member` (its locals and parameters in scope):
    /// an element's property of a list `member` returns.
    pub(in super::super) fn in_member(&self, value: &Value, member: &str, out: &mut Found) {
        let frame = Frame { member: member.to_string(), args: &[], site: None, at: None };
        self.through(value, &frame, &mut Seen::default(), out);
    }

    /// `in_member` over a value that IS a call written at the expression row `site` (a getter's `return this.svc.f(x)`):
    /// the callee's branches are carried (`call_at`). Any other value reads as `in_member` does.
    pub(in super::super) fn in_member_at(&self, value: &Value, member: &str, site: Option<&str>, out: &mut Found) {
        let frame = Frame { member: member.to_string(), args: &[], site: site.map(|s| (value, s)), at: site.map(str::to_string) };
        self.through(value, &frame, &mut Seen::default(), out);
    }

    /// The keys `member` - a method, an arrow property or a `functions` row - returns, entered with no arguments: a function
    /// a template or an instance holds (`tooltip = { text: this.tipText }`) and calls with what only it knows.
    ///
    /// EVERY RETURN'S OWN KEY COUNTS, as a template-called method's does (`key_methods`): the template calls this function
    /// itself, so `if (gone) { return 'a'; } return 'b';` renders either key - and so does a call made FROM it (#122).
    pub(in super::super) fn enter_member(&self, member: &str, out: &mut Found) {
        let mut seen = Seen::default();
        if self.one_return.contains_key(member) {
            return self.enter(member.to_string(), &Value::Null, &mut seen, out, None);
        }
        let frame = Frame { member: member.to_string(), args: &[], site: None, at: None };
        seen.active.push((member.to_string(), format!("{member} []"), usize::MAX));
        for row in self.every.get(member).into_iter().flatten() {
            if let Some(ret) = row.get("value") {
                self.held(ret, &frame, &mut seen, out);
            }
            self.written(row, &frame, &mut seen, out);
        }
    }

    /// The keys of the ONE plain return of `member`, or what each of its several returns hands back through a
    /// call - each (member, arguments) read once.
    ///
    /// AN ARGUMENT THAT IS A TERNARY OF STRINGS IS EACH OF THEM: `k(on ? 'x' : 'y')` over `(n) => t(`q.${n}`)` reads the
    /// callee once per spelling, no condition carried. Left whole, the hole stayed open and an open hole matches every
    /// key under the prefix. Every combination is read, however many.
    fn enter(&self, member: String, call: &Value, seen: &mut Seen, out: &mut Found, site: Option<&str>) {
        let args = call.get("$args").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
        let spelled = spellings(args);
        if spelled.len() > 1 {
            for combo in &spelled {
                self.enter_args(member.clone(), combo, seen, out, site);
            }
            return;
        }
        self.enter_args(member, args, seen, out, site);
    }

    fn enter_args(&self, member: String, args: &[Value], seen: &mut Seen, out: &mut Found, site: Option<&str>) {
        let entry = format!("{member} {}", Value::from(args.to_vec()));
        if let Some(found) = seen.done.get(&entry) {
            return out.extend(compose(found, site));
        }
        let weight = size(args);
        if let Some(i) = seen.active.iter().position(|(m, _, _)| *m == member) {
            let ring: Vec<String> = seen.active[i..].iter().map(|(m, _, _)| m.clone()).collect();
            seen.cyclic.extend(ring);
            let smallest = seen.active.iter().filter(|(m, _, _)| *m == member).map(|(_, _, w)| *w).min().unwrap_or(0);
            if weight > smallest || seen.active.iter().any(|(_, e, _)| *e == entry) {
                return;
            }
        }
        seen.active.push((member.clone(), entry.clone(), weight));
        let mut found = Found::new();
        let frame = Frame { member: member.clone(), args, site: None, at: None };
        if let Some(row) = self.one_return.get(&member) {
            let frame = Frame { at: super::key_fields::text(row, "expression"), ..frame };
            if let Some(ret) = row.get("value") {
                self.held(&self.bind(ret, &frame), &frame, seen, &mut found);
            }
            self.written(row, &frame, seen, &mut found);
        } else {
            for row in self.every.get(&frame.member).into_iter().flatten() {
                let frame = Frame { member: frame.member.clone(), args: frame.args, site: None, at: super::key_fields::text(row, "expression") };
                // A CALLED METHOD'S OWN `if`s RIDE WHAT IT RETURNS under them, placed by its caller (`compose`): a callee's
                // `if (kinds.includes(kind)) return t('a');` restricts `a` to what the caller hands as `kind`.
                let links: Vec<String> = self.branches.chain_of(row).into_iter().filter(|b| self.branches.condition(b).is_some()).map(|b| format!("arg:{b}@")).collect();
                let mut held = Found::new();
                if let Some(ret) = row.get("value") {
                    let bound = self.bind(ret, &frame);
                    // EACH RETURN'S OWN KEY TOO (#122), UNDER THAT RETURN'S OWN BRANCHES: `this.statusKey = this.keyOf()` over
                    // `switch (s) { case A: return 'k.a'; .. }` holds every case's key, each under its case - as the `literal` route
                    // placed it (`key_branches::literal_ways`). Read only through the calls in them, the field held none and its keys
                    // reached no node; read with no chain, a key one case returns lost that case's restriction.
                    value_keys(&bound, &self.branches.chain_of(row), self.keys, &mut found);
                    self.through(&bound, &frame, seen, &mut held);
                }
                self.written(row, &frame, seen, &mut held);
                found.extend(held.into_iter().map(|(key, chain)| (key, [links.clone(), chain].concat())));
            }
        }
        seen.active.pop();
        // IN A CYCLE, OR A BODY A SUBCLASS REPLACES: no condition of this member or below is proved.
        if seen.cyclic.contains(&member) || self.classes.overridden(&member) {
            found.iter_mut().for_each(|(_, chain)| chain.retain(|l| !l.starts_with("arg:")));
        }
        out.extend(compose(&found, site));
        seen.done.insert(entry, found);
    }

    /// A RETURN OF A BARE LOCAL READS EVERY `=` WRITTEN TO IT, not only its initializer: `let s = ''; s = t(k);
    /// return s;` hands back the call, and reading the `''` alone left the key a `literal` no component reached.
    /// The local is the member's own or one an enclosing member holds (`() => second` reads its method's). No
    /// branch is carried - as a method's several returns, they test its parameters - and a local the method
    /// never returns is read by nothing, so a key written to it alone stays unreached. EVERY `.push(x)` ON THE LOCAL
    /// IS ONE MORE ELEMENT (`const out = []; out.push(t('a')); return out;`), in whatever branch it sits - none carried.
    fn written(&self, row: &Row, frame: &Frame<'_>, seen: &mut Seen, out: &mut Found) {
        let Some(name) = super::key_fields::text(row, "source").filter(|s| s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')) else { return };
        let mut at = Some(frame.member.clone());
        while let Some(member) = at {
            let (writes, pushed) = (self.writes.get(&(member.clone(), name.clone())), self.pushes.local.get(&(member.clone(), name.clone())));
            if writes.is_some() || pushed.is_some() {
                for w in writes.into_iter().flatten() {
                    let Some(value) = w.get("value") else { continue };
                    let here = Frame { member: frame.member.clone(), args: frame.args, site: None, at: super::key_fields::text(w, "expression") };
                    self.held(&self.bind(value, &here), &here, seen, out);
                }
                // A PUSHED ARGUMENT HAS NO EXPRESSION ROW OF ITS OWN: a call in it carries no callee branch.
                let unplaced = Frame { member: frame.member.clone(), args: frame.args, site: None, at: None };
                for arg in pushed.into_iter().flatten().filter_map(|p| p.get("args")).filter_map(Value::as_array).flatten() {
                    self.held(&self.bind(arg, &unplaced), &unplaced, seen, out);
                }
                return;
            }
            at = self.parent.get(&member).cloned();
        }
    }

    /// A return's own keys, then the keys it hands back through calls.
    fn held(&self, ret: &Value, frame: &Frame<'_>, seen: &mut Seen, out: &mut Found) {
        value_keys(ret, &[], self.keys, out);
        self.through(ret, frame, seen, out);
    }

    /// A translate call's arguments, another method's or function's return, either side of a `??` or `||`, a ternary's arms.
    fn through(&self, ret: &Value, frame: &Frame<'_>, seen: &mut Seen, out: &mut Found) {
        // AN `await` OF A CALL IS THE CALL: `await this.svc.load()` hands back what `load` resolves to, which is what its
        // returns hold (an `async` method returns the plain value, `Promise.resolve` is read through).
        if let Some(inner) = ret.get("$await") {
            return self.through(inner, frame, seen, out);
        }
        // A TERNARY'S ARMS ARE FOLLOWED LIKE A `??`'S SIDES, no condition carried: `return flag ? t('a') : t('b')`
        // hands back either call, and reading neither left both keys a `literal` no component reached.
        if ret.get("$then").is_some() || ret.get("$else").is_some() {
            for arm in [ret.get("$then"), ret.get("$else")].into_iter().flatten() {
                self.through(arm, frame, seen, out);
            }
            return;
        }
        if matches!(ret.get("$logic").and_then(Value::as_str), Some("??" | "||")) {
            for side in ret.get("$operands").and_then(Value::as_array).into_iter().flatten() {
                self.through(side, frame, seen, out);
            }
            return;
        }
        // A VALUE THAT WRAPS A TRANSLATE CALL HANDS IT BACK: an object's property values, a list's elements that are
        // no objects, a `+` chain's parts, the argument of `Promise.resolve` (the compiler's own lib). No condition
        // carried. The ARGUMENT OF ANY OTHER CALL is not followed - `download(t('k'))` renders nothing in the field -
        // and an object INSIDE a list is `key_loops`'s, read where the loop variable's property is, since reading it
        // here would link a property no template shows.
        match ret {
            Value::Array(items) => {
                items.iter().filter(|i| !plain_object(i)).for_each(|i| self.through(i, frame, seen, out));
                return;
            }
            Value::Object(o) if plain_object(ret) => {
                o.values().for_each(|v| self.through(v, frame, seen, out));
                return;
            }
            // A TEMPLATE LITERAL'S HOLES, evaluated (`$hole_values`, one per hole): `${t('k')}!` hands back the call, a
            // `const` local holding one the same - the evaluator already followed the local to its initializer.
            Value::Object(o) if o.contains_key("$hole_values") => {
                o["$hole_values"].as_array().into_iter().flatten().for_each(|v| self.through(v, frame, seen, out));
                return;
            }
            Value::Object(o) if o.contains_key("$concat") => {
                o["$concat"].as_array().into_iter().flatten().for_each(|v| self.through(v, frame, seen, out));
                return;
            }
            _ => {}
        }
        // A FILTER OR A SLICE OF A LIST KEEPS A SUBSET OF ITS ELEMENTS (#55), as `key_lists::list_of` reads it: what the
        // receiver hands back is what the call may - `[this.home(m)].filter((s) => s !== null)` left the key unreached.
        if matches!(ret.get("$method").and_then(Value::as_str), Some("filter" | "slice")) {
            if let Some(receiver @ Value::Array(_)) = ret.get("$receiver") {
                return self.through(receiver, frame, seen, out);
            }
        }
        if hands_on(ret) {
            ret.get("$args").and_then(Value::as_array).into_iter().flatten().for_each(|a| self.through(a, frame, seen, out));
            return;
        }
        // A LOCAL ARROW OR A FUNCTION THE CALL WAS HANDED IS ENTERED FIRST, even when its name is a configured `i18nCalls`
        // method: `const t = (k) => this.tr.get(`p.${k}`); return t('a');` spells `p.a`, never a key `a`.
        if let Some(name) = ret.get("$call").and_then(Value::as_str).filter(|n| !n.contains('.')) {
            let param = self.params.get(&frame.member).and_then(|ps| ps.iter().position(|p| p == name));
            let handed = param.and_then(|i| frame.args.get(i)).and_then(|a| a.get("$fn_id")).and_then(Value::as_str);
            let local = self.rows.local_row(&frame.member, name).and_then(|row| self.fn_of_local.get(row));
            if let Some(function) = handed.map(str::to_string).or_else(|| local.cloned().filter(|_| param.is_none())) {
                self.enter(function, ret, seen, out, None);
                return;
            }
        }
        if let Some(args) = translated(ret, self.i18n) {
            for arg in args {
                arg_keys(arg, &[], self.keys, out);
            }
            return;
        }
        // A STRING METHOD CHAINED ON A CALL HANDS BACK ITS TEXT: `t('k').replace(..)`, `.trim()`. The extractor evaluates
        // the receiver (`$receiver_value`) when its type is a string; only a method that returns that text reworked is
        // followed (`split` and `includes` return no key), by the lib's own declaration, never the name alone.
        if let Some(receiver) = ret.get("$receiver_value").filter(|_| text_method(ret)) {
            self.through(receiver, frame, seen, out);
            return;
        }
        if let Some(under) = dictionary(ret, self.dict) {
            value_keys(&under, &[], self.keys, out);
            return;
        }
        // THE CALL A FIELD HOLDS ITSELF is placed at its row (#26); A CALL INSIDE what a return holds as `^<row>` (#48).
        let site = match frame.site.filter(|(top, _)| std::ptr::eq(*top, ret)) {
            Some((_, at)) => Some(at.to_string()),
            None => frame.at.as_ref().map(|at| format!("^{at}")),
        };
        self.call_at(Some(ret), site.as_deref(), seen, out);
    }

    /// `v` with every template hole that names a parameter the call handed a string spelled as that string.
    fn bind(&self, v: &Value, frame: &Frame<'_>) -> Value {
        match self.params.get(&frame.member).filter(|_| !frame.args.is_empty()) {
            Some(params) => bind_args(v, params, frame.args),
            None => v.clone(),
        }
    }
}

/// `v` with every template hole naming one of `params` spelled by the argument at its position, when that spells
/// text (`spell`): a string, or a string enum's member.
pub(super) fn bind_args(v: &Value, params: &[String], args: &[Value]) -> Value {
    let string = |hole: &str| params.iter().position(|p| p == hole).and_then(|i| args.get(i)).and_then(spell);
    match v {
        Value::Object(o) if o.contains_key("$template") => {
            let strs = |k: &str| o.get(k).and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect::<Vec<_>>();
            let (pieces, holes) = (strs("$template"), strs("$holes"));
            if pieces.len() != holes.len() + 1 {
                return v.clone();
            }
            let (mut kept, mut open) = (vec![pieces[0].to_string()], Vec::new());
            for (hole, next) in holes.iter().zip(&pieces[1..]) {
                match string(hole) {
                    Some(s) => kept.last_mut().expect("never empty").push_str(&format!("{s}{next}")),
                    None => {
                        open.push(*hole);
                        kept.push((*next).to_string());
                    }
                }
            }
            if open.is_empty() {
                return Value::String(kept.remove(0));
            }
            let mut o = o.clone();
            o.insert("$template".into(), Value::from(kept));
            o.insert("$holes".into(), Value::from(open));
            Value::Object(o)
        }
        // A BARE READ OF A PARAMETER handed a string is that string: `this.home(name)` inside a method called with
        // `'a'` hands `home` the `'a'` too, where it once handed an open hole that spelled every `home.*` key.
        Value::Object(o) if o.get("$kind").and_then(Value::as_str) == Some("Identifier") => {
            match o.get("$expr").and_then(Value::as_str).and_then(string) {
                Some(s) => Value::String(s.to_string()),
                None => v.clone(),
            }
        }
        Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), bind_args(x, params, args))).collect()),
        Value::Array(xs) => Value::Array(xs.iter().map(|x| bind_args(x, params, args)).collect()),
        other => other.clone(),
    }
}

/// Every call of a name, from every statement row: (the member it sits in, its argument list).
fn calls_by_name(store: &Store<'_>) -> std::collections::HashMap<String, Vec<(String, Vec<Value>)>> {
    fn walk(v: &Value, member: &str, out: &mut std::collections::HashMap<String, Vec<(String, Vec<Value>)>>) {
        match v {
            Value::Object(o) => {
                if let Some(name) = o.get("$call").and_then(Value::as_str).filter(|n| !n.contains('.')) {
                    let args = o.get("$args").and_then(Value::as_array).cloned().unwrap_or_default();
                    out.entry(name.to_string()).or_default().push((member.to_string(), args));
                }
                o.values().for_each(|x| walk(x, member, out));
            }
            Value::Array(xs) => xs.iter().for_each(|x| walk(x, member, out)),
            _ => {}
        }
    }
    let mut out: std::collections::HashMap<String, Vec<(String, Vec<Value>)>> = std::collections::HashMap::new();
    for table in ["calls", "assignments", "returns", "locals"] {
        for r in store.table(table).iter() {
            let member = super::key_fields::id_of(r, "member").unwrap_or_default();
            if let Some(callee) = super::key_fields::text(r, "callee").filter(|c| !c.contains('.')) {
                let args = r.get("args").and_then(Value::as_array).cloned().unwrap_or_default();
                out.entry(callee).or_default().push((member.clone(), args));
            }
            r.get("args").into_iter().chain(r.get("value")).for_each(|v| walk(v, &member, &mut out));
        }
    }
    out
}

/// `functions` row of a `const` local arrow -> the argument lists EVERY call of it hands, ternaries of strings
/// expanded, when each of them spells every parameter (`spell`). An arrow with a call that does not - a variable, a
/// property, a numeric member, a callback's item - or with no call found is NOT here: its own template stays a
/// wildcard, since nothing says which keys it spells. Calls are found by the local's name inside the member that holds
/// it (and the functions nested in that member), never by text.
pub(super) fn spelled_arrows(store: &Store<'_>, params: &std::collections::HashMap<String, Vec<String>>) -> std::collections::HashMap<String, Vec<Vec<Value>>> {
    let mut out = std::collections::HashMap::new();
    let locals = store.table("locals");
    let arrows: Vec<(String, String, String)> = locals
        .iter()
        .filter_map(|l| Some((l.get("value")?.get("$fn_id")?.as_str()?.to_string(), super::key_fields::text(l, "name")?, super::key_fields::id_of(l, "member")?)))
        .collect();
    if arrows.is_empty() {
        return out;
    }
    let parent: std::collections::HashMap<String, String> = store
        .table("functions")
        .iter()
        .filter_map(|f| Some((super::key_fields::id_of(f, "id")?, super::key_fields::id_of(f, "parent")?)))
        .collect();
    let calls = calls_by_name(store);
    for (function, name, home) in arrows {
        let Some(ps) = params.get(&function).filter(|p| !p.is_empty()) else { continue };
        let inside = |member: &str| {
            let mut at = member.to_string();
            for _ in 0..64 {
                if at == home {
                    return true;
                }
                match parent.get(&at) {
                    Some(up) => at = up.clone(),
                    None => return false,
                }
            }
            false
        };
        let mine: Vec<&Vec<Value>> = calls.get(&name).into_iter().flatten().filter(|(m, _)| inside(m)).map(|(_, a)| a).collect();
        let lists: Vec<Vec<Value>> = mine.iter().flat_map(|a| spellings(a)).collect();
        if !lists.is_empty() && lists.iter().all(|l| ps.iter().enumerate().all(|(i, _)| l.get(i).is_some_and(|a| spell(a).is_some()))) {
            out.insert(function, lists);
        }
    }
    out
}

#[cfg(test)]
#[path = "key_follow_tests.rs"]
mod tests;
