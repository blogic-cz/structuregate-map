//! WHAT A CALLED METHOD OR FUNCTION HANDS BACK, read for the keys it holds - see `key_fields::callee_keys`.

use super::key_branches::value_keys;
use super::store::Row;
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

/// The member a call entered and the arguments it handed over.
struct Frame<'v> {
    member: String,
    args: &'v [Value],
}

type Seen = std::collections::HashSet<String>;
type Found = Vec<(String, Vec<String>)>;

impl Returned<'_, '_> {
    /// The member a call's `$target` names, entered with the call's arguments.
    pub(super) fn call(&self, value: Option<&Value>, seen: &mut Seen, out: &mut Found) {
        let Some(value) = value else { return };
        let Some(member) = value.get("$target").and_then(|t| self.rows.member_named(t)) else { return };
        self.enter(member, value, seen, out);
    }

    /// The keys of the ONE plain return of `member`, or what each of its several returns hands back through a
    /// call - each (member, arguments) read once.
    fn enter(&self, member: String, call: &Value, seen: &mut Seen, out: &mut Found) {
        let args = call.get("$args").and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default();
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
        let method = ret.get("$target").and_then(|t| t.get("name")).and_then(Value::as_str);
        if method.is_some_and(|m| self.i18n.iter().any(|c| c == m)) {
            for arg in ret.get("$args").and_then(Value::as_array).into_iter().flatten() {
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
        let Some(params) = self.params.get(&frame.member).filter(|_| !frame.args.is_empty()) else { return v.clone() };
        let string = |hole: &str| params.iter().position(|p| p == hole).and_then(|i| frame.args.get(i)).and_then(Value::as_str);
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
            Value::Object(o) => Value::Object(o.iter().map(|(k, x)| (k.clone(), self.bind(x, frame))).collect()),
            Value::Array(xs) => Value::Array(xs.iter().map(|x| self.bind(x, frame)).collect()),
            other => other.clone(),
        }
    }
}
