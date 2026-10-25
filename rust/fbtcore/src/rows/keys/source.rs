//! WHICH SOURCE A MAP DESCRIBES (#130): `_meta` says what a stored entity key was taken from - per root `source_commit:<root>`,
//! `source_branch:<root>` and `source_dirty:<root>` (1 when tracked files differ from that commit; none of the three outside a git
//! repository), `gate_version` (the release tag the exe was built from, `dev` for a local build) and `gate_build` (the digest of
//! what produces the rows, which tells two local builds apart). `built_at` (UTC, ISO 8601) moves only when the passes over the
//! finished rows ran again - a run that changed nothing leaves the database untouched, as every `_meta` key here is written only
//! when it differs.

use rusqlite::Connection;
use std::path::Path;

/// The release tag the exe was built from - the release workflow's `STRUCTUREGATE_VERSION` - or `dev`.
pub const GATE_VERSION: &str = match option_env!("STRUCTUREGATE_VERSION") {
    Some(v) => v,
    None => "dev",
};

/// Write the source state into `_meta`; `derived` when the passes over the finished rows ran (not replayed).
pub fn source_state(conn: &Connection, roots: &[String], build: &str, derived: bool) -> rusqlite::Result<()> {
    let keep = |key: &str, value: &str| -> rusqlite::Result<()> {
        if crate::rows::meta::get(conn, key).as_deref() == Some(value) {
            return Ok(());
        }
        crate::rows::meta::put(conn, key, value).map_err(|e| rusqlite::Error::InvalidParameterName(format!("{e:#}")))
    };
    keep("gate_version", GATE_VERSION)?;
    keep("gate_build", build)?;
    for root in roots {
        let place = Path::new(root);
        let git = |args: &[&str]| crate::sources::git_lines(place, args);
        let Some(commit) = git(&["rev-parse", "HEAD"]).into_iter().next() else { continue };
        keep(&format!("source_commit:{root}"), &commit)?;
        keep(&format!("source_branch:{root}"), git(&["rev-parse", "--abbrev-ref", "HEAD"]).first().map_or("", String::as_str))?;
        // TRACKED FILES ONLY, under that root: an untracked scratch file is no change to what the map read from the commit.
        let dirty = !git(&["status", "--porcelain", "--untracked-files=no", "--", "."]).is_empty();
        keep(&format!("source_dirty:{root}"), if dirty { "1" } else { "0" })?;
    }
    if derived {
        keep("built_at", &now_utc())?;
    }
    Ok(())
}

/// The current UTC time as `YYYY-MM-DDTHH:MM:SSZ`, from the system clock (civil date by days since the epoch).
fn now_utc() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (y, m, d) = civil(days);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rest / 3600, rest % 3600 / 60, rest % 60)
}

/// Days since 1970-01-01 as (year, month, day), the proleptic Gregorian calendar.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_day_count_is_its_calendar_date() {
        assert_eq!(super::civil(0), (1970, 1, 1));
        assert_eq!(super::civil(20_386), (2025, 10, 25));
        assert_eq!(super::civil(11_016), (2000, 2, 29));
    }
}
