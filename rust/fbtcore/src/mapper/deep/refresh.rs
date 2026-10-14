//! WHAT A DEEP MAP RUN DID, SAID ON EVERY RUN AND KEPT. A refresh after a pull took minutes on a large tree and
//! printed one line - the rows and how many files were re-read - with no trace asked for: which half took the time and
//! why it ran could not be found out afterwards. So every run now says how long each step took, and keeps that, with
//! each half's own account of why it ran (`Collector::refresh`), in `_meta` as `last_refresh` - one JSON value, replaced
//! by every run that did work (a run that re-read nothing and replayed the passes keeps the one before), readable with `--map-query <db> --sql "SELECT value FROM _meta WHERE key = 'last_refresh'"`.
//!
//! NO KEY READS IT: the passes over the finished rows key on `counter:`, `setup:` and `schema` (`derived.rs`), and a
//! payload half on the tree, so a value that moves on every run moves nothing else. Its timings come from the clock
//! here, never from the trace, which may be off.

use super::super::protocol::Collector;
use serde_json::{json, Value};
use std::path::Path;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub const KEY: &str = "last_refresh";

/// The run's own clock: when it started, and each step it timed, in order.
pub struct Clock {
    started: Instant,
    steps: Vec<(String, u64)>,
}

impl Clock {
    pub fn start() -> Clock {
        Clock { started: Instant::now(), steps: Vec::new() }
    }

    /// One step, timed. A step timed twice (the plain half's two turns) adds up.
    pub fn time<T>(&mut self, name: &str, work: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let out = work();
        self.add(name, started.elapsed().as_millis() as u64);
        out
    }

    pub fn add(&mut self, name: &str, ms: u64) {
        match self.steps.iter_mut().find(|(n, _)| n == name) {
            Some((_, total)) => *total += ms,
            None => self.steps.push((name.to_string(), ms)),
        }
    }
}

/// THE RUN'S ACCOUNT: one note with every step's time, and `_meta.last_refresh`. `replayed` says whether the passes over
/// the finished rows were said again rather than run.
pub fn said(into: &mut Collector, db: &str, clock: Clock, replayed: bool) {
    let took = clock.started.elapsed().as_millis() as u64;
    let steps: Vec<String> = clock.steps.iter().map(|(name, ms)| format!("{name} {}", seconds(*ms))).collect();
    into.notes.push(format!("the deep map took {}: {}", seconds(took), steps.join(", ")));
    // A RUN THAT DID NOTHING - no file re-read, the passes replayed, no error - KEEPS THE LAST ONE'S ACCOUNT: the
    // database and the trace beside it are left untouched, as they were before either existed. Rewriting them each
    // turn moved a file inside the tree on every run (a gate's pass cache then never hits) and replaced the slow run's
    // account - the one worth keeping - with a run that took a second.
    let worked = into.reread > 0 || !replayed || !into.errors.is_empty();
    if !worked {
        crate::trace::keep_previous_run();
        return;
    }
    if !Path::new(db).is_file() {
        return;
    }
    let mut kept = json!({
        "when": utc(SystemTime::now()),
        "took_ms": took,
        "steps_ms": clock.steps.iter().map(|(name, ms)| (name.clone(), Value::from(*ms))).collect::<serde_json::Map<_, _>>(),
        "passes": if replayed { "replayed" } else { "ran" },
        "reread": into.reread.max(0),
        "errors": into.errors.len(),
        "trace": crate::trace::last_run(db),
    });
    for (half, facts) in &into.refresh {
        kept[half.as_str()] = facts.clone();
    }
    let written = rusqlite::Connection::open(db).and_then(|conn| {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS _meta (key TEXT, value TEXT)")?;
        conn.execute("DELETE FROM _meta WHERE key = ?1", [KEY])?;
        conn.execute("INSERT INTO _meta (key, value) VALUES (?1, ?2)", [KEY, &kept.to_string()])
    });
    if let Err(why) = written {
        into.notes.push(format!("the run's account was not kept in _meta.{KEY} - {why}"));
    }
}

/// `_meta.roots`: EVERY `--root` the map was built from, as a JSON array of absolute folders in the order given. `root`
/// is the first alone, written by every half's store (`rows/store.rs`), and a map of `src` and `test` read as a map of
/// `src`. Kept beside it, never instead: every reader already takes `root`. WRITTEN ONLY WHEN IT DIFFERS, so a run that
/// did nothing leaves the database untouched (see `said`); a file holding no `_meta` is not a map yet and is left alone.
pub fn roots(into: &mut Collector, db: &str, roots: &[(String, String)]) {
    keep(into, db, "roots", &roots_value(roots));
}

/// `_meta.outputs`: what the run WROTE INSIDE ITS OWN TREE besides the database - the `--map` JSON - as a JSON array of
/// absolute paths. A later run maps that file like any other, and it is rewritten after the database every time, so
/// the stale note (`query/fresh.rs`) would call it changed after every run; it reads this list and skips them.
pub fn outputs(into: &mut Collector, db: &str, outputs: &[String]) {
    let paths: Vec<String> = outputs
        .iter()
        .map(|out| std::path::absolute(out).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| out.clone()))
        .collect();
    keep(into, db, "outputs", &Value::from(paths).to_string());
}

/// One `_meta` key set to `wanted`, WRITTEN ONLY WHEN IT DIFFERS (see `roots`).
fn keep(into: &mut Collector, db: &str, key: &str, wanted: &str) {
    if !Path::new(db).is_file() {
        return;
    }
    let written = rusqlite::Connection::open(db).and_then(|conn| {
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let meta: i64 = conn.query_row("SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = '_meta'", [], |r| r.get(0))?;
        if meta == 0 {
            return Ok(());
        }
        let held: Option<String> = conn
            .query_row("SELECT value FROM _meta WHERE key = ?1", [key], |r| r.get(0))
            .or_else(|e| if matches!(e, rusqlite::Error::QueryReturnedNoRows) { Ok(None) } else { Err(e) })?;
        if held.as_deref() == Some(wanted) {
            return Ok(());
        }
        conn.execute("DELETE FROM _meta WHERE key = ?1", [key])?;
        conn.execute("INSERT INTO _meta (key, value) VALUES (?1, ?2)", [key, wanted]).map(|_| ())
    });
    if let Err(why) = written {
        into.notes.push(format!("the map's {key} were not kept in _meta.{key} - {why}"));
    }
}

/// The roots as `_meta.roots` holds them: absolute, as `root` is written.
fn roots_value(roots: &[(String, String)]) -> String {
    let dirs: Vec<String> = roots
        .iter()
        .map(|(_, dir)| std::path::absolute(dir).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| dir.clone()))
        .collect();
    Value::from(dirs).to_string()
}

/// Milliseconds as seconds with one decimal - `0.4 s`, `125.3 s`.
fn seconds(ms: u64) -> String {
    format!("{}.{} s", ms / 1000, (ms % 1000) / 100)
}

/// A time as ISO 8601 UTC to the second, from the days since 1970 (Howard Hinnant's `civil_from_days`).
fn utc(at: SystemTime) -> String {
    let secs = at.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rest / 3600, rest % 3600 / 60, rest % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_time_is_written_as_iso_utc() {
        assert_eq!(utc(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        assert_eq!(utc(UNIX_EPOCH + Duration::from_secs(1_791_385_560)), "2026-10-07T15:06:00Z");
        assert_eq!(utc(UNIX_EPOCH + Duration::from_secs(951_782_400)), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn every_root_is_kept_in_order_and_rewritten_only_when_it_moved() {
        let dir = std::env::temp_dir().join(format!("sg-roots-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = dir.join("map.sqlite");
        let _ = std::fs::remove_file(&db);
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch("CREATE TABLE _meta (key TEXT, value TEXT); INSERT INTO _meta VALUES ('root', '/t/src');").unwrap();
        drop(conn);
        let given = vec![("src/".to_string(), "/t/src".to_string()), ("test/".to_string(), "/t/test".to_string())];
        let mut into = Collector::new();
        let held = |db: &Path| -> Vec<String> {
            let conn = rusqlite::Connection::open(db).unwrap();
            let mut stmt = conn.prepare("SELECT value FROM _meta WHERE key = 'roots'").unwrap();
            stmt.query_map([], |r| r.get(0)).unwrap().flatten().collect()
        };
        let db_text = db.to_string_lossy().into_owned();
        roots(&mut into, &db_text, &given);
        roots(&mut into, &db_text, &given);
        let once = held(&db);
        assert_eq!(once.len(), 1, "{once:?}");
        let listed: Vec<String> = serde_json::from_str(&once[0]).unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed[0].ends_with("src") && listed[1].ends_with("test"), "{listed:?}");
        roots(&mut into, &db_text, &given[..1]);
        let moved: Vec<String> = serde_json::from_str(&held(&db)[0]).unwrap();
        assert_eq!(moved.len(), 1);
        assert!(into.notes.is_empty(), "{:?}", into.notes);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_step_timed_twice_adds_up_and_seconds_keep_one_decimal() {
        let mut clock = Clock::start();
        clock.add("csharp", 1_250);
        clock.add("csharp", 100);
        assert_eq!(clock.steps, vec![("csharp".to_string(), 1_350)]);
        assert_eq!(seconds(1_350), "1.3 s");
        assert_eq!(seconds(125_312), "125.3 s");
    }
}
