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
    /// Member or `functions` row id -> its parameters' names.
    pub(super) params: &'r std::collections::HashMap<String, Vec<String>>,
    /// `locals` row id -> the `functions` row of the arrow it holds.
    pub(super) fn_of_local: &'r std::collections::HashMap<String, String>,
    /// (member id, local name) -> every `=` written to that local, in source order.
    pub(super) writes: &'r std::collections::HashMap<(String, String), Vec<&'a Row>>,
    /// `functions` row id -> the member or function it sits in.
    pub(super) parent: &'r std::collections::HashMap<String, String>,
}

/// The arguments of a call of a configured `i18nCalls` method, None for any other value.
pub(super) fn translated<'v>(v: &'v Value, i18n: &[String]) -> Option<&'v [Value]> {
    let method = v.get("$target").and_then(|t| t.get("name")).and_then(Value::as_str)?;
    i18n.iter().any(|c| c == method).then(|| v.get("$args").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default())
}

/// An object literal: no key of it is a row marker (`$call`, `$template`, `$cond_expr` ...).
fn plain_object(v: &Value) -> bool {
    v.as_object().is_some_and(|o| o.keys().all(|k| !k.starts_with('$')))
}

/// `Promise.resolve(x)` of the compiler's own lib: `$target` names `resolve` in `typescript/lib/lib.*.d.ts`, compared
/// by path pieces.
fn is_promise_resolve(v: &Value) -> bool {
    let Some(target) = v.get("$target") else { return false };
    let file = target.get("file").and_then(Value::as_str).unwrap_or_default();
    let pieces: Vec<&str> = file.split(['/', '\\']).collect();
    target.get("name").and_then(Value::as_str) == Some("resolve")
        && pieces.last().is_some_and(|f| f.starts_with("lib."))
        && pieces.windows(2).any(|w| w == ["typescript", "lib"])
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

/// Most argument lists one call's ternaries of strings expand to.
const MAX_SPELLINGS: usize = 16;

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

/// The argument lists a call stands for: the product of each ternary-of-strings argument's arms, or the list
/// itself when none is one or the product passes `MAX_SPELLINGS`.
fn spellings(args: &[Value]) -> Vec<Vec<Value>> {
    let mut lists = vec![Vec::new()];
    for arg in args {
        let options = arms(arg).filter(|_| spell(arg).is_none()).unwrap_or_else(|| vec![arg.clone()]);
        if lists.len() * options.len() > MAX_SPELLINGS {
            return vec![args.to_vec()];
        }
        lists = lists.into_iter().flat_map(|l| options.iter().map(move |o| [l.clone(), vec![o.clone()]].concat())).collect();
    }
    lists
}

/// The member a call entered and the arguments it handed over.
struct Frame<'v> {
    member: String,
    args: &'v [Value],
}

type Seen = std::collections::HashSet<String>;
type Found = Vec<(String, Vec<String>)>;

impl Returned<'_, '_> {
    /// The member - or, failing that, the free function - a call's `$target` names, entered with the call's
    /// arguments. A FREE FUNCTION IS ENTERED LIKE A METHOD: `this.c = label('a')` over `export const label = (n) =>
    /// `demo.${n}`` hands the field `demo.a`, and reading only class members left every key such a function builds
    /// a `literal` no component reached. No condition is carried, and each (callee, arguments) is read once.
    pub(super) fn call(&self, value: Option<&Value>, seen: &mut Seen, out: &mut Found) {
        let Some(value) = value else { return };
        let Some(target) = value.get("$target") else { return };
        let Some(member) = self.rows.member_named(target).or_else(|| self.rows.function_named(target)) else { return };
        self.enter(member, value, seen, out);
    }

    /// The keys a value holds through the calls in it, read inside `member` (its locals and parameters in scope):
    /// an element's property of a list `member` returns.
    pub(in super::super) fn in_member(&self, value: &Value, member: &str, out: &mut Found) {
        let frame = Frame { member: member.to_string(), args: &[] };
        self.through(value, &frame, &mut Seen::new(), out);
    }

    /// The keys of the ONE plain return of `member`, or what each of its several returns hands back through a
    /// call - each (member, arguments) read once.
    ///
    /// AN ARGUMENT THAT IS A TERNARY OF STRINGS IS EACH OF THEM: `k(on ? 'x' : 'y')` over `(n) => t(`q.${n}`)` reads the
    /// callee once per spelling, no condition carried. Left whole, the hole stayed open and an open hole matches every
    /// key under the prefix. Past `MAX_SPELLINGS` combinations the call is read as it was.
    fn enter(&self, member: String, call: &Value, seen: &mut Seen, out: &mut Found) {
        let args = call.get("$args").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
        let spelled = spellings(args);
        if spelled.len() > 1 {
            for combo in &spelled {
                self.enter_args(member.clone(), combo, seen, out);
            }
            return;
        }
        self.enter_args(member, args, seen, out);
    }

    fn enter_args(&self, member: String, args: &[Value], seen: &mut Seen, out: &mut Found) {
        if seen.len() > 64 || !seen.insert(format!("{member} {}", Value::from(args.to_vec()))) {
            return;
        }
        let frame = Frame { member, args };
        if let Some(row) = self.one_return.get(&frame.member) {
            if let Some(ret) = row.get("value") {
                self.held(&self.bind(ret, &frame), &frame, seen, out);
            }
            self.written(row, &frame, seen, out);
        } else {
            for row in self.every.get(&frame.member).into_iter().flatten() {
                if let Some(ret) = row.get("value") {
                    self.through(&self.bind(ret, &frame), &frame, seen, out);
                }
                self.written(row, &frame, seen, out);
            }
        }
    }

    /// A RETURN OF A BARE LOCAL READS EVERY `=` WRITTEN TO IT, not only its initializer: `let s = ''; s = t(k);
    /// return s;` hands back the call, and reading the `''` alone left the key a `literal` no component reached.
    /// The local is the member's own or one an enclosing member holds (`() => second` reads its method's). No
    /// branch is carried - as a method's several returns, they test its parameters - and a local the method
    /// never returns is read by nothing, so a key written to it alone stays unreached.
    fn written(&self, row: &Row, frame: &Frame<'_>, seen: &mut Seen, out: &mut Found) {
        let Some(name) = super::key_fields::text(row, "source").filter(|s| s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$')) else { return };
        let mut at = Some(frame.member.clone());
        while let Some(member) = at {
            if let Some(writes) = self.writes.get(&(member.clone(), name.clone())) {
                for write in writes {
                    if let Some(value) = write.get("value") {
                        self.held(&self.bind(value, frame), frame, seen, out);
                    }
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
            Value::Object(o) if o.contains_key("$concat") => {
                o["$concat"].as_array().into_iter().flatten().for_each(|v| self.through(v, frame, seen, out));
                return;
            }
            _ => {}
        }
        if is_promise_resolve(ret) {
            ret.get("$args").and_then(Value::as_array).into_iter().flatten().for_each(|a| self.through(a, frame, seen, out));
            return;
        }
        if let Some(args) = translated(ret, self.i18n) {
            for arg in args {
                value_keys(arg, &[], self.keys, out);
            }
            return;
        }
        if let Some(name) = ret.get("$call").and_then(Value::as_str).filter(|n| !n.contains('.')) {
            let param = self.params.get(&frame.member).and_then(|ps| ps.iter().position(|p| p == name));
            let handed = param.and_then(|i| frame.args.get(i)).and_then(|a| a.get("$fn_id")).and_then(Value::as_str);
            let local = self.rows.local_row(&frame.member, name).and_then(|row| self.fn_of_local.get(row));
            if let Some(function) = handed.map(str::to_string).or_else(|| local.cloned().filter(|_| param.is_none())) {
                self.enter(function, ret, seen, out);
                return;
            }
        }
        self.call(Some(ret), seen, out);
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
