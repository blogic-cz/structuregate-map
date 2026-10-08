//! THE PATH CONDITION OF A RENDER SITE, EVALUATED against one occurrence's literal - the
//! three-valued logic `gate_guards` reads a directive's code path with.
//!
//! A condition is `True`/`False` where the literal decides it, `Unk` where only the running
//! application can, and an `Atom` where it is a membership of a template expression in a
//! list of constants. `necessary` is what a condition cannot be true without.

use super::astreads::unwrap;
use indexmap::IndexMap;
use serde_json::Value;

pub(crate) fn k(n: &Value) -> Option<&str> {
    n.get("k").and_then(|v| v.as_str())
}

pub(crate) fn name(n: &Value) -> Option<&str> {
    n.get("name").and_then(|v| v.as_str())
}

/// A bare name nothing resolved: a local, a parameter, a callback's element.
pub(crate) fn bare(n: &Value) -> Option<&str> {
    let implicit = n.get("receiver").and_then(k) == Some("Implicit");
    (k(n) == Some("Read") && implicit && n.get("target").is_none()).then(|| name(n))?
}

/// A membership of a template expression in a list of one enum's constants.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Atom {
    pub enum_id: String,
    pub values: Vec<String>,
    pub dim: Value,
    pub holds: bool,
}

/// A condition as far as the literal decides it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum F {
    True,
    False,
    Unk,
    Atom(Atom),
    And(Vec<F>),
    Or(Vec<F>),
}

pub(crate) fn not(f: F) -> F {
    match f {
        F::True => F::False,
        F::False => F::True,
        F::Unk => F::Unk,
        F::Atom(a) => F::Atom(Atom { holds: !a.holds, ..a }),
        F::And(xs) => F::Or(xs.into_iter().map(not).collect()),
        F::Or(xs) => F::And(xs.into_iter().map(not).collect()),
    }
}

/// The atoms `f` cannot be true without, or None when the literal makes it false.
pub(crate) fn necessary(f: &F) -> Option<Vec<Atom>> {
    match f {
        F::True | F::Unk => Some(Vec::new()),
        F::False => None,
        F::Atom(a) => Some(vec![a.clone()]),
        F::And(xs) => {
            let mut out = Vec::new();
            for x in xs {
                out.extend(necessary(x)?);
            }
            Some(out)
        }
        F::Or(xs) => {
            let mut sides = xs.iter().filter_map(necessary);
            let first = sides.next()?;
            Some(sides.fold(first, |held, next| held.into_iter().filter(|a| next.contains(a)).collect()))
        }
    }
}

/// What an expression holds once the literal is filled in.
#[derive(Debug, Clone)]
pub(crate) enum Sym {
    Absent,
    Bool(bool),
    /// A list, with its members when every item is a constant of one enum.
    List(usize, Option<(String, Vec<String>)>),
    /// The config object itself.
    Present,
    /// A value only the running application knows, as the template spells it.
    Dim(Value),
    Unk,
}

pub(crate) struct Eval<'a> {
    pub env: &'a IndexMap<String, Sym>,
    pub aliases: &'a IndexMap<String, String>,
    pub param: Option<&'a str>,
}

impl Eval<'_> {
    fn value(&self, n: &Value) -> Sym {
        let receiver = n.get("receiver");
        match k(n) {
            Some("Read") | Some("SafeRead") => {
                if let Some(p) = self.param
                    && bare(n) == Some(p)
                {
                    return Sym::Present;
                }
                let member = match receiver.and_then(k) {
                    Some("This") => name(n).and_then(|f| self.aliases.get(f)).cloned(),
                    _ if receiver.and_then(bare).is_some() && receiver.and_then(bare) == self.param => {
                        name(n).map(str::to_string)
                    }
                    _ => None,
                };
                match member {
                    Some(m) => self.env.get(&m).cloned().unwrap_or(Sym::Absent),
                    None => Sym::Unk,
                }
            }
            Some("Literal") => match n.get("v") {
                Some(Value::Bool(b)) => Sym::Bool(*b),
                Some(Value::Null) | None => Sym::Absent,
                Some(Value::Number(x)) => Sym::Bool(x.as_f64() != Some(0.0)),
                Some(Value::String(s)) => Sym::Bool(!s.is_empty()),
                _ => Sym::Unk,
            },
            _ => Sym::Unk,
        }
    }

    /// `S` of `x === S`, where `x` is the callback's element: a bare name, the other side a
    /// value the literal leaves to the application.
    fn element(&self, cb: &Value) -> Option<Value> {
        let body = cb.get("returns")?.as_array()?.as_slice();
        let [b] = body else { return None };
        if k(b) != Some("Binary") || !matches!(b.get("op")?.as_str()?, "===" | "==") {
            return None;
        }
        let (l, r) = (b.get("left")?, b.get("right")?);
        let pick = |el: &Value, s: &Value| bare(el).and(match self.value(s) {
            Sym::Dim(d) => Some(d),
            _ => None,
        });
        pick(l, r).or_else(|| pick(r, l))
    }

    /// A membership call over a known list: `L.includes(S)`, `L.some(...)`, `L.find(...)`.
    fn member_call(&self, n: &Value) -> Option<(String, Vec<String>, Value, bool)> {
        if k(n) != Some("Call") {
            return None;
        }
        let callee = n.get("receiver")?;
        let Sym::List(_, Some((en, vals))) = self.value(callee.get("receiver")?) else { return None };
        let [arg] = n.get("args")?.as_array()?.as_slice() else { return None };
        let method = name(callee)?;
        let dim = match method {
            "includes" => match self.value(arg) {
                Sym::Dim(d) => d,
                _ => return None,
            },
            "some" | "find" => self.element(arg)?,
            _ => return None,
        };
        Some((en, vals, dim, method == "find"))
    }

    pub(crate) fn truth(&self, n: &Value) -> F {
        let n = unwrap(n);
        match k(n) {
            Some("Not") => return not(n.get("expr").map(|e| self.truth(e)).unwrap_or(F::Unk)),
            Some("Binary") => {
                let (Some(op), Some(l), Some(r)) = (n.get("op").and_then(|o| o.as_str()), n.get("left"), n.get("right")) else {
                    return F::Unk;
                };
                match op {
                    "&&" => return F::And(vec![self.truth(l), self.truth(r)]),
                    "||" => return F::Or(vec![self.truth(l), self.truth(r)]),
                    "===" | "==" | "!==" | "!=" => {
                        let eq = matches!(op, "===" | "==");
                        let (call, other) = if bare(r) == Some("undefined") { (l, r) } else { (r, l) };
                        if bare(other) == Some("undefined")
                            && let Some((en, values, dim, true)) = self.member_call(call)
                        {
                            return F::Atom(Atom { enum_id: en, values, dim, holds: !eq });
                        }
                        return F::Unk;
                    }
                    ">" | ">=" | "<" | "<=" => {
                        let len = (name(l) == Some("length")).then(|| l.get("receiver").map(|x| self.value(x))).flatten();
                        let lit = r.get("v").and_then(|v| v.as_f64());
                        if let (Some(Sym::List(len, _)), Some(lit)) = (len, lit) {
                            let len = len as f64;
                            let b = match op { ">" => len > lit, ">=" => len >= lit, "<" => len < lit, _ => len <= lit };
                            return if b { F::True } else { F::False };
                        }
                        return F::Unk;
                    }
                    _ => return F::Unk,
                }
            }
            Some("Call") => {
                if let Some((en, values, dim, find)) = self.member_call(n) {
                    // A `find` is truthy only if what it finds is: `x === 0` finds a falsy member.
                    return if find { F::Unk } else { F::Atom(Atom { enum_id: en, values, dim, holds: true }) };
                }
                return F::Unk;
            }
            _ => {}
        }
        match self.value(n) {
            Sym::Absent => F::False,
            Sym::Bool(b) => if b { F::True } else { F::False },
            Sym::List(..) | Sym::Present => F::True,
            Sym::Dim(_) | Sym::Unk => F::Unk,
        }
    }
}
