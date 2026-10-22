//! METRICS: where the time went inside a stage, finer than the trace keeps - per file, per pass, per kind of compiler
//! question. COMPILED IN ONLY WITH THE `metrics` FEATURE (`dotnet build -p:Metrics=true`): a release has no call, no
//! branch and no tally, because every call site is `metric!` (`trace/mod.rs`), which is its bare body without it, the C#
//! half's calls are `[Conditional("SG_METRICS")]` (`CsMetrics.cs`), and node is handed no `SG_METRICS` file to write.
//!
//! EVERY NAME IS A TALLY - how many, how long in all, the slowest and which item it was - written at the end of the run
//! as a `metric: <name>` span of `structuregate.timing = breakdown` (it explains its stage, never adds to it; a total
//! summed over threads may be longer than the run). An item slower than `SG_METRICS_SLOW_MS` (50 by default) is also a
//! `metric item: <name>` span of its own, the slowest `SLOWEST` of them. TIMING ONLY: nothing here may change a row.

use serde_json::Value;
use std::collections::BTreeMap;
use std::ffi::c_char;
use std::sync::Mutex;
use std::time::Duration;

/// EVERY RUST ALLOCATION, COUNTED: mimalloc underneath, the bytes held now kept in one atomic. The `metrics` build's
/// allocator only (`lib.rs`); `sample_memory` writes the count beside the trace, so a phase's memory can be split into
/// rust's own and the rest (.NET's heap, SQLite's allocations through libc, the runtime).
pub struct Counting;

static HELD: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// The most `HELD` reached since the last `mark`.
static PEAK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

fn grew(size: usize) {
    let now = HELD.fetch_add(size, std::sync::atomic::Ordering::Relaxed) + size;
    PEAK.fetch_max(now, std::sync::atomic::Ordering::Relaxed);
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

unsafe impl std::alloc::GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        let p = unsafe { mimalloc::MiMalloc.alloc(layout) };
        if !p.is_null() {
            grew(layout.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: std::alloc::Layout) {
        unsafe { mimalloc::MiMalloc.dealloc(p, layout) };
        HELD.fetch_sub(layout.size(), std::sync::atomic::Ordering::Relaxed);
    }
    unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
        let p = unsafe { mimalloc::MiMalloc.alloc_zeroed(layout) };
        if !p.is_null() {
            grew(layout.size());
        }
        p
    }
    unsafe fn realloc(&self, p: *mut u8, layout: std::alloc::Layout, size: usize) -> *mut u8 {
        let q = unsafe { mimalloc::MiMalloc.realloc(p, layout, size) };
        if !q.is_null() {
            grew(size);
            HELD.fetch_sub(layout.size(), std::sync::atomic::Ordering::Relaxed);
        }
        q
    }
}

/// The bytes rust holds now - for a phase-only harness that measures one step.
pub fn held() -> usize {
    HELD.load(std::sync::atomic::Ordering::Relaxed)
}

/// WHAT HOLDS THE BYTES (`retained!`, `trace/mod.rs`): every table the TypeScript closure's store read and every column it
/// decoded, by the bytes they took (`store: <what>`), and every structure a step of the closure built and kept, by the bytes it
/// kept BESIDE whatever the store took while it was built - one thread runs the closure up to `key_reach`, so the deltas are
/// its own. `mark` snapshots what is alive at the end of a step: `held()`, the store's share and each structure not yet
/// `gone`. Written as `metric mem: ..` spans (`drain`) and printed by a phase-only harness (`memory_report`).
#[derive(Default)]
struct Ledger {
    /// Each structure: (name, bytes, alive).
    held: Vec<(String, i64, bool)>,
    /// `<table>` / `<table>.<column>`: the bytes the store took for it.
    stored: BTreeMap<String, i64>,
    /// The same, per (what, the step that took it).
    by_step: BTreeMap<(String, u32), i64>,
    /// Per `mark`: (step, held, the most held since the mark before, the store's share, the process's anonymous memory,
    /// the structures alive and their bytes).
    marks: Vec<(String, i64, i64, i64, i64, Vec<(String, i64)>)>,
}

static LEDGER: Mutex<Ledger> = Mutex::new(Ledger { held: Vec::new(), stored: BTreeMap::new(), by_step: BTreeMap::new(), marks: Vec::new() });
static STORE: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
/// Every byte a `built` structure was charged, so one built inside another is charged once - to the inner.
static CHARGED: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);

/// `held()`, the store's share and what was charged so far, taken before a structure is built (`retained!`).
pub struct Before(i64, i64, i64);

impl Before {
    pub fn now() -> Before {
        Before(held() as i64, STORE.load(std::sync::atomic::Ordering::Relaxed), CHARGED.load(std::sync::atomic::Ordering::Relaxed))
    }

    /// What the structure built since `now` kept: the growth, less what the store took and the structures built inside it kept.
    pub fn built(self, name: &str) {
        let store = STORE.load(std::sync::atomic::Ordering::Relaxed) - self.1;
        let inner = CHARGED.load(std::sync::atomic::Ordering::Relaxed) - self.2;
        let kept = held() as i64 - self.0 - store - inner;
        CHARGED.fetch_add(kept, std::sync::atomic::Ordering::Relaxed);
        if let Ok(mut ledger) = LEDGER.lock() {
            ledger.held.retain(|(n, _, _)| n != name);
            ledger.held.push((name.to_string(), kept, true));
        }
    }

    /// The store took what was allocated since `now`, under `what` (a table read, a column decoded).
    pub fn stored(self, what: &str) {
        let grew = held() as i64 - self.0;
        STORE.fetch_add(grew, std::sync::atomic::Ordering::Relaxed);
        let Ok(mut ledger) = LEDGER.lock() else { return };
        *ledger.by_step.entry((what.to_string(), step())).or_default() += grew;
        match ledger.stored.get_mut(what) {
            Some(bytes) => *bytes += grew,
            None => {
                ledger.stored.insert(what.to_string(), grew);
            }
        }
    }
}

/// The store let every row go (`Store::release`): its share is nothing again.
pub fn store_released() {
    STORE.store(0, std::sync::atomic::Ordering::Relaxed);
}

/// A structure dropped: no later `mark` lists it.
pub fn gone(name: &str) {
    if let Ok(mut ledger) = LEDGER.lock() {
        ledger.held.iter_mut().filter(|(n, _, _)| n == name).for_each(|e| e.2 = false);
    }
}

/// THE STEP THE CLOSURE IS IN: how many `mark`s came before - a read is charged to the mark that ends its step.
static STEP: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn step() -> u32 {
    STEP.load(std::sync::atomic::Ordering::Relaxed)
}

/// Per table and per column, the first and the last step that read it: what a step could let go after it.
type Steps = std::rc::Rc<[std::cell::Cell<(u32, u32)>]>;

thread_local! {
    static READS: std::cell::RefCell<Vec<(String, Vec<String>, Steps)>> = const { std::cell::RefCell::new(Vec::new()) };
    static TABLES: std::cell::RefCell<BTreeMap<String, (u32, u32)>> = const { std::cell::RefCell::new(BTreeMap::new()) };
}

pub fn register_reads(table: &str, names: Vec<String>, steps: Steps) {
    READS.with(|r| r.borrow_mut().push((table.to_string(), names, steps)));
}

pub fn asked_table(table: &str) {
    let now = step();
    TABLES.with(|t| {
        let mut t = t.borrow_mut();
        let entry = t.entry(table.to_string()).or_insert((u32::MAX, 0));
        *entry = (entry.0.min(now), entry.1.max(now));
    });
}

/// Every table asked for and every column read, with the steps of its first and last read, by the marks' names.
pub fn reading() -> String {
    let names: Vec<String> = LEDGER.lock().map(|l| l.marks.iter().map(|m| m.0.clone()).collect()).unwrap_or_default();
    let name = |s: u32| names.get(s as usize).cloned().unwrap_or_else(|| format!("after the last mark ({s})"));
    let mut out = String::new();
    TABLES.with(|t| for (table, (first, last)) in t.borrow().iter() {
        out.push_str(&format!("TABLE {table}: {} .. {}\n", name(*first), name(*last)));
    });
    let mut columns: BTreeMap<String, (u32, u32)> = BTreeMap::new();
    READS.with(|r| for (table, cols, steps) in r.borrow().iter() {
        for (col, s) in cols.iter().zip(steps.iter()) {
            let (first, last) = s.get();
            if first == u32::MAX { continue }
            let entry = columns.entry(format!("{table}.{col}")).or_insert((u32::MAX, 0));
            *entry = (entry.0.min(first), entry.1.max(last));
        }
    });
    for (column, (first, last)) in columns {
        out.push_str(&format!("COLUMN {column}: {} .. {}\n", name(first), name(last)));
    }
    out
}

/// What is alive at the end of `step`.
pub fn mark(step: &str) {
    let Ok(mut ledger) = LEDGER.lock() else { return };
    let alive = ledger.held.iter().filter(|(_, _, a)| *a).map(|(n, b, _)| (n.clone(), *b)).collect();
    let store = STORE.load(std::sync::atomic::Ordering::Relaxed);
    let peak = PEAK.swap(held(), std::sync::atomic::Ordering::Relaxed) as i64;
    STEP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    ledger.marks.push((step.to_string(), held() as i64, peak, store, anonymous(), alive));
}

/// The process's anonymous memory (`RssAnon`), 0 where the system does not say.
fn anonymous() -> i64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    status.lines().find_map(|l| l.strip_prefix("RssAnon:")).and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<i64>().ok()).map_or(0, |kb| kb * 1024)
}

/// Every mark, then the store's tables and columns, largest first - for a harness to print.
#[cfg_attr(not(test), allow(dead_code))]
pub fn memory_report() -> String {
    let reading = reading();
    let Ok(ledger) = LEDGER.lock() else { return String::new() };
    let mb = |b: i64| b as f64 / 1_048_576.0;
    let mut out = String::new();
    for (step, held, peak, store, anon, alive) in &ledger.marks {
        out.push_str(&format!("MARK {step}: held {:.0} MB (peak in the step {:.0} MB), store {:.0} MB, process anon {:.0} MB\n",
            mb(*held), mb(*peak), mb(*store), mb(*anon)));
        let mut alive = alive.clone();
        alive.sort_by(|a, b| b.1.cmp(&a.1));
        for (name, bytes) in alive.iter().filter(|(_, b)| b.abs() >= 1 << 20) {
            out.push_str(&format!("    {name}: {:.0} MB\n", mb(*bytes)));
        }
    }
    let mut stored: Vec<(&String, &i64)> = ledger.stored.iter().collect();
    stored.sort_by(|a, b| b.1.cmp(a.1));
    for (name, bytes) in stored.into_iter().filter(|(_, b)| **b >= 1 << 20) {
        out.push_str(&format!("store: {name}: {:.0} MB\n", mb(*bytes)));
    }
    let mut by_step: Vec<(&(String, u32), &i64)> = ledger.by_step.iter().collect();
    by_step.sort_by(|a, b| b.1.cmp(a.1));
    let step_name = |s: u32| ledger.marks.get(s as usize).map_or_else(|| format!("after the last mark ({s})"), |m| m.0.clone());
    for ((what, step), bytes) in by_step.into_iter().filter(|(_, b)| **b >= 1 << 20) {
        out.push_str(&format!("took: {what} in {}: {:.0} MB\n", step_name(*step), mb(*bytes)));
    }
    out.push_str(&reading);
    out
}

/// THE BYTES RUST HOLDS, twice a second, as `<epoch ns>\t<bytes>` lines in `file` - from now until the process ends.
pub fn sample_memory(file: std::path::PathBuf) {
    let _ = std::thread::Builder::new().name("metrics: memory".into()).spawn(move || {
        use std::io::Write;
        let Ok(mut out) = std::fs::File::create(&file) else { return };
        loop {
            let ns = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
            let _ = writeln!(out, "{ns}\t{}", HELD.load(std::sync::atomic::Ordering::Relaxed));
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    });
}

/// How many slow items a run keeps - the slowest.
const SLOWEST: usize = 2000;

#[derive(Default)]
struct Tally {
    count: u64,
    total_ns: u128,
    max_ns: u128,
    max_item: String,
}

static TALLIES: Mutex<BTreeMap<String, Tally>> = Mutex::new(BTreeMap::new());
static SLOW: Mutex<Vec<(String, String, u128)>> = Mutex::new(Vec::new());

fn slow_ns() -> u128 {
    std::env::var("SG_METRICS_SLOW_MS").ok().and_then(|v| v.parse::<u128>().ok()).unwrap_or(50) * 1_000_000
}

/// One item of `name`, timed by the caller.
pub fn record(name: &str, item: &str, took: Duration) {
    let ns = took.as_nanos();
    merge(name, 1, ns, ns, item);
    if ns >= slow_ns() {
        keep_slow(name, item, ns);
    }
}

fn merge(name: &str, count: u64, total_ns: u128, max_ns: u128, max_item: &str) {
    let Ok(mut tallies) = TALLIES.lock() else { return };
    let tally = tallies.entry(name.to_string()).or_default();
    tally.count += count;
    tally.total_ns += total_ns;
    if max_ns > tally.max_ns {
        tally.max_ns = max_ns;
        tally.max_item = max_item.to_string();
    }
}

fn keep_slow(name: &str, item: &str, ns: u128) {
    let Ok(mut slow) = SLOW.lock() else { return };
    slow.push((name.to_string(), item.to_string(), ns));
    if slow.len() > SLOWEST * 2 {
        slow.sort_by(|a, b| b.2.cmp(&a.2));
        slow.truncate(SLOWEST);
    }
}

/// WHAT A HOST TALLIED ITSELF - the C# half through `fbt_metric`, node through its `SG_METRICS` file: one entry per
/// name, `{"name", "count", "total_ms", "max_ms", "max_item", "slow": [[item, ms], ..]}`.
pub fn merge_host(entries: &Value) {
    let ns = |v: &Value| (v.as_f64().unwrap_or(0.0) * 1_000_000.0) as u128;
    for entry in entries.as_array().into_iter().flatten() {
        let Some(name) = entry["name"].as_str() else { continue };
        merge(name, entry["count"].as_u64().unwrap_or(0), ns(&entry["total_ms"]), ns(&entry["max_ms"]), entry["max_item"].as_str().unwrap_or(""));
        for slow in entry["slow"].as_array().into_iter().flatten() {
            keep_slow(name, slow[0].as_str().unwrap_or(""), ns(&slow[1]));
        }
    }
}

/// The C# half's tallies, a JSON array as `merge_host` reads it. Called only from `[Conditional("SG_METRICS")]` code.
///
/// # Safety
/// `json` must be null or a NUL-terminated string that stays alive for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fbt_metric(json: *const c_char) {
    let _ = std::panic::catch_unwind(|| {
        if let Some(text) = unsafe { crate::in_string(json) }
            && let Ok(entries) = serde_json::from_str::<Value>(&text)
        {
            merge_host(&entries);
        }
    });
}

/// Every tally and slow item, emptied: `(name, ns, attributes)` for the spans `finish` writes.
pub(super) fn drain() -> Vec<(String, u128, Vec<(String, Value)>)> {
    let mut spans = Vec::new();
    let breakdown = || ("structuregate.timing".to_string(), Value::from("breakdown"));
    if let Ok(mut tallies) = TALLIES.lock() {
        for (name, t) in std::mem::take(&mut *tallies) {
            spans.push((format!("metric: {name}"), t.total_ns, vec![
                breakdown(),
                ("structuregate.metric.count".into(), Value::from(t.count)),
                ("structuregate.metric.total_ms".into(), Value::from((t.total_ns / 1_000_000) as u64)),
                ("structuregate.metric.max_ms".into(), Value::from((t.max_ns / 1_000_000) as u64)),
                ("structuregate.metric.max_item".into(), Value::from(t.max_item)),
            ]));
        }
    }
    if let Ok(mut ledger) = LEDGER.lock() {
        let ledger = std::mem::take(&mut *ledger);
        for (step, held, peak, store, anon, alive) in ledger.marks {
            let mut attributes = vec![breakdown(), ("structuregate.metric.held".into(), Value::from(held)), ("structuregate.metric.peak".into(), Value::from(peak)),
                ("structuregate.metric.store".into(), Value::from(store)),
                ("structuregate.metric.anon".into(), Value::from(anon))];
            attributes.extend(alive.into_iter().map(|(name, bytes)| (format!("structuregate.metric.alive.{name}"), Value::from(bytes))));
            spans.push((format!("metric mem: {step}"), 0, attributes));
        }
        for (name, bytes) in ledger.stored {
            spans.push((format!("metric mem: store: {name}"), 0, vec![breakdown(), ("structuregate.metric.bytes".into(), Value::from(bytes))]));
        }
    }
    if let Ok(mut slow) = SLOW.lock() {
        let mut items = std::mem::take(&mut *slow);
        items.sort_by(|a, b| b.2.cmp(&a.2));
        items.truncate(SLOWEST);
        for (name, item, ns) in items {
            spans.push((format!("metric item: {name}"), ns, vec![breakdown(), ("structuregate.metric.item".into(), Value::from(item))]));
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tally_counts_sums_keeps_the_slowest_and_drains_once() {
        record("unit: probe", "a", Duration::from_millis(3));
        record("unit: probe", "b", Duration::from_millis(80));
        merge_host(&serde_json::json!([{ "name": "unit: probe", "count": 2, "total_ms": 10.0, "max_ms": 7.0, "max_item": "c",
            "slow": [["d", 60.0]] }]));
        let spans = drain();
        let tally = spans.iter().find(|(n, _, _)| n == "metric: unit: probe").expect("the tally");
        let fact = |k: &str| tally.2.iter().find(|(key, _)| key == k).map(|(_, v)| v.clone()).unwrap();
        assert_eq!(fact("structuregate.metric.count"), serde_json::json!(4));
        assert_eq!(fact("structuregate.metric.total_ms"), serde_json::json!(93));
        assert_eq!(fact("structuregate.metric.max_item"), serde_json::json!("b"));
        let slow: Vec<&str> = spans.iter().filter(|(n, _, _)| n == "metric item: unit: probe")
            .map(|(_, _, a)| a[1].1.as_str().unwrap()).collect();
        assert_eq!(slow, ["b", "d"], "the slow items, slowest first");
        assert!(drain().iter().all(|(n, _, _)| !n.contains("unit: probe")), "drained once");
    }
}
