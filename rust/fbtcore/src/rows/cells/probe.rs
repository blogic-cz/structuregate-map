//! LAB PROBE (not committed): which child tables a closure decodes, how often, from where.
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

type Entry = (u64, HashSet<String>, u128, HashMap<String, u64>);
thread_local! {
    static P: RefCell<HashMap<String, Entry>> = RefCell::new(HashMap::new());
    static C: RefCell<HashMap<String, (u64, u128)>> = RefCell::new(HashMap::new());
}

pub fn on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("SG_PROBE").is_ok())
}

pub fn record(child: &str, owner: &str, nanos: u128) {
    P.with(|p| {
        let mut p = p.borrow_mut();
        let e = p.entry(child.to_string()).or_default();
        e.0 += 1;
        e.2 += nanos;
        let repeat = !e.1.insert(owner.to_string());
        if e.0 % 7 == 0 {
            let bt = std::backtrace::Backtrace::force_capture().to_string();
            let mut caller = Vec::new();
            for line in bt.lines() {
                let l = line.trim();
                if l.starts_with("at ./src/rows/ts/") && !l.contains("/store/") {
                    caller.push(l.trim_start_matches("at ./src/rows/ts/").to_string());
                    if caller.len() == 1 {
                        break;
                    }
                }
            }
            *e.3.entry(format!("{}{}", if repeat { "R " } else { "  " }, caller.join(" < "))).or_default() += 1;
            C.with(|c| { let mut c = c.borrow_mut(); let x = c.entry(format!("{} {}{}", child, if repeat { "R " } else { "  " }, caller.join(" < "))).or_default(); x.0 += 7; x.1 += nanos * 7; });
        }
    });
}

pub fn report() -> String {
    P.with(|p| {
        let p = p.borrow();
        let mut out = String::new();
        let mut v: Vec<_> = p.iter().collect();
        v.sort_by_key(|(_, e)| std::cmp::Reverse(e.2));
        for (child, (n, owners, ns, callers)) in v {
            out.push_str(&format!("{child}: {n} decodes, {} distinct, {} ms\n", owners.len(), ns / 1_000_000));
            let mut c: Vec<_> = callers.iter().collect();
            c.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
            for (k, n) in c.iter().take(12) {
                out.push_str(&format!("    {n:6} {k}\n"));
            }
        }
        C.with(|c| {
            let c = c.borrow();
            let mut v: Vec<_> = c.iter().collect();
            v.sort_by_key(|(_, x)| std::cmp::Reverse(x.1));
            out.push_str("BY CALLER (x7 estimated)\n");
            for (k, (n, ns)) in v.iter().take(40) {
                out.push_str(&format!("{:6} ms {n:8} {k}\n", ns / 1_000_000));
            }
        });
        out
    })
}
