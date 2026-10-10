//! THE TYPESCRIPT HALF IN FLIGHT: node parses on a thread of its own while the halves after it - python, rust, C# -
//! store theirs, and its rows are stored once they have. On a large tree node's parse and Roslyn's compile were
//! minutes each, one waiting for the other, on a machine with cores to spare; and the python host started beside
//! the file map was thrown away whenever the TypeScript half stored before python's turn.
//!
//! THE IDS ARE THE HAZARD. Every half numbers its rows from the counters the database recorded, and nine prefixes are
//! shared - so a half that numbers from the counters it was handed at the START and stores at the END would hand out
//! the ids the halves beside it took meanwhile. The flight numbers in a LANE instead: every counter it is handed is
//! `LANE` above the recorded one, and a prefix never recorded starts at `LANE` (`floor`), so the halves beside it
//! number below and it numbers above. Before its rows are stored the lane is checked - a half beside it that handed
//! out more than `LANE` ids of one prefix crossed into it - and a crossed lane is parsed again, in its turn.
//!
//! NOT FLOWN where the numbering must hold still or there is nothing to share: a half ALONE in its database restarts
//! its ids (`half::alone`), and a database being REBUILT has no counters to lane. Nor beside a C# half with more than
//! `BESIDE` files to read again: both held at once cost gigabytes for a minute. It then runs in its turn, first.
//!
//! THE DATABASE IS READ BEFORE IT IS WRITTEN: the state, the plan's carried rows and the line counts are read on the
//! thread before node starts, and nothing beside it writes until it says so. A run asked again over every hop
//! (`MAP-RETRY`) reads them again, so it is asked in its turn after the flight lands.

use super::super::protocol::Collector;
use super::ts::{self, Parsed, Planned};
use super::Deep;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

/// The id room the flight numbers in, above what the database recorded. Ten times the most ids of one prefix a large
/// tree's whole C# half was seen to hold; crossing it costs a second parse, never a shared id.
pub(super) const LANE: i64 = 10_000_000;

/// THE MOST C# FILES TO RE-READ BESIDE A FLIGHT. Over a large tree's whole C# half the two ran barely faster and took
/// gigabytes more at their peak - node's rows and every Roslyn compilation held at once - and each slowed the other on four
/// cores; beside a pull's worth of moved files the parse is node's whole time saved.
pub(super) const BESIDE: usize = 1000;

/// What became of the half: flown beside the others (whether the tree had no workspace - the one outcome after which
/// the plain half answers instead - and how long it took, the flight and its landing), done without a launch (the
/// same), or to be launched in its turn, with the workspace the last run reported.
pub(super) enum Flight {
    Flown { absent: bool, ms: u64 },
    Done(bool),
    Grounded(Option<String>),
}

/// What came back from the thread.
struct Landed {
    into: Collector,
    parsed: Option<Parsed>,
    absent: bool,
    carried: Option<u64>,
    ms: u64,
}

/// THE HALF IN FLIGHT beside `beside`, which runs on this thread meanwhile - when `fly` lets it. Unless it was flown,
/// `beside` has not run.
pub(super) fn run(into: &mut Collector, deep: &Deep, db: &str, root: &str, key: &dyn Fn(Option<&str>) -> Option<String>, fly: bool,
    beside: &mut dyn FnMut(&mut Collector)) -> Flight {
    let fe_kept = match ts::skipped(into, deep, db, root, key) {
        Ok(absent) => return Flight::Done(absent),
        Err(fe) => fe,
    };
    if !fly {
        return Flight::Grounded(fe_kept);
    }
    let Some(parse) = deep.scripts.get("tsrows-node").cloned() else { return Flight::Grounded(fe_kept) };
    let handing = crate::trace::stage("typescript: read and hand over the state");
    let (state, recorded) = match laned(into, deep, db) {
        Err(()) => return Flight::Done(false),
        Ok(None) => return Flight::Grounded(fe_kept),
        Ok(Some(laned)) => laned,
    };
    let work = ts::Work::new();
    let written = std::fs::write(&work.state, state);
    drop(handing);
    if written.is_err() {
        work.clean();
        return Flight::Grounded(fe_kept);
    }
    let anchor = crate::trace::anchor();
    let (read, reading) = std::sync::mpsc::channel::<()>();
    let landed = std::thread::scope(|scope| {
        let flying = scope.spawn(|| {
            let _adopted = crate::trace::adopt(anchor);
            let stage = crate::trace::stage("deep: typescript");
            stage.set("structuregate.flight", "parse");
            let timed = Instant::now();
            let mut own = Collector::new();
            let mut carried = None;
            let planned = ts::plan(&mut own, deep, &parse, root, db, &work, false, &mut carried);
            let absent = planned == Planned::Absent;
            let parsed = (planned == Planned::Full || planned == Planned::Partial).then(|| {
                let carry = (planned == Planned::Partial).then_some(&work.carry);
                ts::parse_tree(&mut own, deep, &parse, root, db, &work, carry, false, &|| {
                    let _ = read.send(());
                })
            });
            drop(read);
            Landed { into: own, parsed, absent, carried, ms: timed.elapsed().as_millis() as u64 }
        });
        // NOTHING BESIDE IT WRITES until the thread has read what it reads - or has ended without saying so.
        let _ = reading.recv();
        beside(into);
        flying.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    });
    let landing = Instant::now();
    let stage = crate::trace::stage("deep: typescript");
    stage.set("structuregate.flight", "store");
    let (errors, notes) = (into.errors.len(), into.notes.len());
    let clean = landed.into.errors.is_empty();
    absorb(into, landed.into);
    let mut fe = None;
    let mut absent = landed.absent;
    let again = match landed.parsed {
        Some(Parsed::Rows) => match crossed(db, &recorded) {
            Some(prefix) => {
                into.notes.push(format!(
                    "the typescript half parses again, in its turn: the halves beside it handed out more than {LANE} id(s) of `{prefix}`"));
                work.clean();
                absent = ts::launched(into, deep, db, root, &mut fe, &[false, true]);
                false
            }
            None => ts::store(into, db, root, &work.rows, landed.carried, &mut fe),
        },
        Some(Parsed::Retry) => true,
        _ => false,
    };
    work.clean();
    if again {
        absent = ts::launched(into, deep, db, root, &mut fe, &[true]);
    }
    let absence = ts::absence(&into.notes[notes..]);
    ts::settle(db, key, absent, fe, fe_kept, clean && into.errors.len() == errors, absence);
    drop(stage);
    Flight::Flown { absent, ms: landed.ms + landing.elapsed().as_millis() as u64 }
}

/// The state node is handed, in the lane, with the counters the database recorded; None when the half must not fly,
/// Err (said) when the state could not be read.
#[allow(clippy::type_complexity)]
fn laned(into: &mut Collector, deep: &Deep, db: &str) -> Result<Option<(String, BTreeMap<String, i64>)>, ()> {
    let text = ts::state(into, deep, db).ok_or(())?;
    let Ok(mut state) = serde_json::from_str::<Value>(&text) else { return Ok(None) };
    if state["alone"] == Value::Bool(true) || state["rebuild"] == Value::Bool(true) {
        return Ok(None);
    }
    let Some(counters) = state["counters"].as_object_mut() else { return Ok(None) };
    let recorded: BTreeMap<String, i64> = counters.iter().map(|(prefix, n)| (prefix.clone(), n.as_i64().unwrap_or(0))).collect();
    for n in counters.values_mut() {
        *n = Value::from(n.as_i64().unwrap_or(0) + LANE);
    }
    state["floor"] = Value::from(LANE);
    Ok(Some((state.to_string(), recorded)))
}

/// The first prefix the halves beside the flight numbered into its lane with.
fn crossed(db: &str, recorded: &BTreeMap<String, i64>) -> Option<String> {
    crate::rows::store::counters(Path::new(db)).into_iter()
        .find(|(prefix, now)| *now > recorded.get(prefix).copied().unwrap_or(0) + LANE)
        .map(|(prefix, _)| prefix)
}

/// What the thread said, in the run's own account.
fn absorb(into: &mut Collector, own: Collector) {
    into.errors.extend(own.errors);
    into.notes.extend(own.notes);
    into.halves.extend(own.halves);
    for (half, why) in own.refresh {
        into.refresh.entry(half).or_insert(why);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A database whose `_meta` records `counters`.
    fn recording(name: &str, counters: &[(&str, i64)]) -> String {
        let db = std::env::temp_dir().join(format!("fbt-flight-{name}-{}.sqlite", std::process::id()));
        let _ = std::fs::remove_file(&db);
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch("CREATE TABLE _meta (key TEXT, value TEXT)").unwrap();
        for (prefix, n) in counters {
            conn.execute("INSERT INTO _meta (key, value) VALUES (?1, ?2)", (format!("counter:{prefix}"), n.to_string())).unwrap();
        }
        db.to_string_lossy().into_owned()
    }

    #[test]
    fn a_lane_holds_while_the_halves_beside_it_stay_below_it() {
        let db = recording("held", &[("f", 30 + LANE), ("x", 7)]);
        let recorded = BTreeMap::from([("f".to_string(), 30)]);
        assert_eq!(crossed(&db, &recorded), None);
    }

    #[test]
    fn a_lane_is_crossed_by_one_id_past_it_even_of_a_prefix_never_recorded() {
        let recorded = BTreeMap::from([("f".to_string(), 30)]);
        assert_eq!(crossed(&recording("past", &[("f", 31 + LANE)]), &recorded), Some("f".to_string()));
        assert_eq!(crossed(&recording("new", &[("f", 31), ("q", LANE + 1)]), &recorded), Some("q".to_string()));
    }
}
