//! WHETHER A MAP IS CURRENT, which is what `--map-if-stale` asks before it starts a single parser.
//!
//! Three things make a map stale: a source newer than it, a file added or gone (a move keeps its mtime,
//! so the file SET is hashed into the map's head and compared), and THE GATE ITSELF being newer than the
//! map. The third was missing: a publish changes no source and no file set, so a tree that did not move
//! kept answering from the old exe's rules - new sections, new findings, fixed lenses - until someone
//! edited a file. A hook exited 0 over a database a day behind the exe that was supposed to write it.
//!
//! A FOURTH: the `--map-baseline` written after the map. The ratchet's findings (DYNAMIC, UNREAD) were judged
//! against the baseline as it was, and a kept map is now CHECKED (`check.rs`), so its findings must still be true.
//!
//! Pure, so the rule is tested without an exe to touch: the black-box harness runs the dll under
//! `dotnet.exe`, whose mtime is nobody's publish.

use std::path::Path;
use std::time::SystemTime;

/// Why the map is not current. In the order they are checked, so the message names the first reason.
#[derive(Debug, PartialEq)]
pub enum Why {
    Source,
    Inventory,
    Baseline,
    Exe,
}

/// `None` when nothing has to be parsed again. A map that cannot be dated is stale.
pub fn why(
    map: Option<SystemTime>,
    newest_source: SystemTime,
    exe: Option<SystemTime>,
    recorded: &str,
    inventory: &str,
    baseline: Option<SystemTime>,
) -> Option<Why> {
    let Some(map) = map else { return Some(Why::Source) };
    if map < newest_source {
        return Some(Why::Source);
    }
    if recorded != inventory {
        return Some(Why::Inventory);
    }
    if baseline.is_some_and(|written| map < written) {
        return Some(Why::Baseline);
    }
    if exe.is_some_and(|built| map < built) {
        return Some(Why::Exe);
    }
    None
}

/// When the running gate was deployed: the mtime of its own file. Every consumer's copy is a hard link to
/// one release, so a publish moves this on all of them at once. `None` when the exe cannot be dated, which
/// leaves the other two rules deciding, as before.
pub fn exe_modified() -> Option<SystemTime> {
    std::env::current_exe().ok().and_then(|p| std::fs::metadata(p).ok()).and_then(|m| m.modified().ok())
}

/// WHICH FILES A MAP COVERS, as one hash written FIRST in the map, so `--map-if-stale` reads it without
/// parsing the rest. A MOVE KEEPS A FILE'S MTIME and a deletion leaves nothing to be newer, so the set of
/// files is what is compared.
pub(super) fn inventory(walked: &[String]) -> String {
    let mut sorted = walked.to_vec();
    sorted.sort();
    blake3::hash(sorted.join("\n").as_bytes()).to_hex()[..16].to_string()
}

/// What the map at `path` recorded - its first property, read off the head of the file - or "".
pub(super) fn recorded(path: &Path) -> String {
    // THE HEAD ONLY: the map runs to megabytes, and its first property is all this asks.
    let mut bytes = Vec::with_capacity(4096);
    let Ok(file) = std::fs::File::open(path) else { return String::new() };
    if std::io::Read::read_to_end(&mut std::io::Read::take(file, 4096), &mut bytes).is_err() {
        return String::new();
    }
    let head = String::from_utf8_lossy(&bytes).into_owned();
    let rest = head.trim_start();
    let Some(rest) = rest.strip_prefix('{') else { return String::new() };
    let Some(rest) = rest.trim_start().strip_prefix("\"inventory\"") else { return String::new() };
    let Some(rest) = rest.trim_start().strip_prefix(':') else { return String::new() };
    let Some(rest) = rest.trim_start().strip_prefix('"') else { return String::new() };
    rest.find('"').map(|end| rest[..end].to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(seconds: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
    }

    #[test]
    fn a_map_newer_than_every_source_and_the_exe_is_current() {
        assert_eq!(why(Some(at(100)), at(90), Some(at(80)), "abc", "abc", Some(at(95))), None);
    }

    #[test]
    fn a_gate_published_after_the_map_makes_it_stale_though_no_source_moved() {
        assert_eq!(why(Some(at(100)), at(90), Some(at(110)), "abc", "abc", None), Some(Why::Exe));
    }

    #[test]
    fn a_map_baseline_written_after_the_map_makes_it_stale() {
        assert_eq!(why(Some(at(100)), at(90), Some(at(80)), "abc", "abc", Some(at(105))), Some(Why::Baseline));
    }

    #[test]
    fn a_source_or_a_file_set_still_decides_first_and_an_undated_exe_decides_nothing() {
        assert_eq!(why(Some(at(100)), at(120), Some(at(110)), "abc", "abc", None), Some(Why::Source));
        assert_eq!(why(Some(at(100)), at(90), Some(at(110)), "abc", "xyz", None), Some(Why::Inventory));
        assert_eq!(why(Some(at(100)), at(90), None, "abc", "abc", None), None);
        assert_eq!(why(None, at(90), None, "abc", "abc", None), Some(Why::Source));
    }
}
