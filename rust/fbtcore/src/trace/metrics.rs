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
