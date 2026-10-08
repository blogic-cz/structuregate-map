//! `--cat FRAGMENT [--lines A-B]`: one file's source out of `file_text`, numbered.
//! A child of `query`: what it reads of the lenses' helpers is theirs, by `super::*`.

use super::*;

/// How many candidates a refused fragment lists before it says how many more there are.
const LISTED: usize = 20;

/// Prints the file and returns true, or prints why not and returns false - which fails the run.
/// THE LINE IS PRINTED WHOLE, whatever `--width` says: a source viewer that cuts `formatRowHe...[+40 chars]` has
/// to be read twice.
pub(super) fn lens(db: &Connection, out: &mut String, fragment: &str, span: &str) -> Result<bool> {
    let sql = "SELECT path, content FROM file_text WHERE path LIKE ?";
    if !has_file_text(db) {
        let _ = writeln!(out, "no source for {} - this database has no file_text: it was built without FTS5, see --tables", quoted(fragment));
        return Ok(false);
    }
    let Some((spelling, found)) = rooted(db, out, sql, fragment)? else {
        unstored(db, out, fragment);
        return Ok(false);
    };
    let paths: Vec<String> = found.iter().map(|row| cell(&row[0])).collect();
    let Some(chosen) = pick(&paths, &spelling) else {
        let _ = writeln!(out, "{} matches {} files - name more of its path:", quoted(fragment), paths.len());
        let mut sorted = paths.clone();
        sorted.sort();
        for path in sorted.iter().take(LISTED) {
            let _ = writeln!(out, "  {path}");
        }
        if sorted.len() > LISTED {
            let _ = writeln!(out, "  ... and {} more", sorted.len() - LISTED);
        }
        return Ok(false);
    };
    let path = &paths[chosen];
    let content = cell(&found[chosen][1]);
    // A LAST NEWLINE ENDS THE LAST LINE, it does not start one more: `files.lines` counts the same way.
    let lines: Vec<&str> = content.strip_suffix('\n').unwrap_or(&content).split('\n').collect();
    let Some((first, last)) = range(span, lines.len()) else {
        let _ = writeln!(
            out,
            "{path} has {} lines - --lines {span} is not a range in it (FIRST-LAST, from 1)",
            lines.len()
        );
        return Ok(false);
    };
    let _ = writeln!(out, "{path}  lines {first}-{last} of {}", lines.len());
    for n in first..=last {
        let _ = writeln!(out, "{n:>5}  {}", lines[n - 1]);
    }
    Ok(true)
}

fn has_file_text(db: &Connection) -> bool {
    db.query_row("SELECT 1 FROM sqlite_master WHERE name = 'file_text'", [], |_| Ok(())).is_ok()
}

/// THE ONE FILE A FRAGMENT NAMES, or none when it names several. `LIKE %fragment%` alone took whichever row came
/// first: `OrderRecordService.cs` printed `OldOrderRecordService.cs` while a file of exactly that
/// name was beside it. So the whole path wins, then a path the fragment ENDS on at a folder boundary, then the
/// only substring match - and two candidates at the best of those levels are refused, never guessed between.
fn pick(paths: &[String], fragment: &str) -> Option<usize> {
    let wanted = fragment.replace('\\', "/").to_lowercase();
    let lowered: Vec<String> = paths.iter().map(|p| p.to_lowercase()).collect();
    let tail = format!("/{}", wanted.trim_start_matches('/'));
    let levels: [&dyn Fn(&str) -> bool; 3] = [&|p| p == wanted, &|p| p.ends_with(&tail), &|_| true];
    for level in levels {
        let hits: Vec<usize> = (0..paths.len()).filter(|&i| level(&lowered[i])).collect();
        match hits.len() {
            0 => continue,
            1 => return Some(hits[0]),
            _ => return None,
        }
    }
    None
}

/// `--lines A-B` (or `A`, or nothing for the whole file) as a range INSIDE the file. One past its end used to print
/// `lines 63-42 of 42` and no text, which reads as a file that is there and empty where the line was asked.
fn range(span: &str, count: usize) -> Option<(usize, usize)> {
    if span.is_empty() {
        return Some((1, count));
    }
    let (first, last) = match span.split_once('-') {
        Some((a, b)) => (a.trim().parse().ok()?, b.trim().parse().ok()?),
        None => {
            let n: usize = span.trim().parse().ok()?;
            (n, n)
        }
    };
    (first >= 1 && first <= last && last <= count).then_some((first, last))
}

/// No source matched: say whether the FILE is mapped, which is a map built before its half stored source text, or
/// is not, which is a fragment that names nothing. The old hint blamed FTS5 for both, over a database that had it.
fn unstored(db: &Connection, out: &mut String, fragment: &str) {
    let mapped = query(db, "SELECT path FROM files WHERE path LIKE ? LIMIT 1", &[&format!("%{fragment}%")])
        .ok()
        .and_then(|(_, rows)| rows.first().map(|row| cell(&row[0])));
    match mapped {
        Some(path) => {
            let _ = writeln!(
                out,
                "no source for {} - {path} is mapped, but its half stored no text for it: rebuild the map with this structuregate",
                quoted(fragment)
            );
        }
        None => {
            let _ = writeln!(out, "no source for {} - no file in this map matches it", quoted(fragment));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[&str]) -> Vec<String> {
        list.iter().map(|p| p.to_string()).collect()
    }

    #[test]
    fn a_whole_file_name_beats_a_longer_name_ending_the_same() {
        let found = paths(&[
            "src/Old/OldOrderRecordService.cs",
            "Sales/src/Core/OrderRecordService.cs",
            "Sales/src/Domain/IOrderRecordService.cs",
        ]);
        assert_eq!(pick(&found, "OrderRecordService.cs"), Some(1));
    }

    #[test]
    fn a_fragment_naming_several_files_is_refused() {
        let found = paths(&["a/Startup.cs", "b/Startup.cs"]);
        assert_eq!(pick(&found, "Startup.cs"), None);
        assert_eq!(pick(&found, "b/Startup.cs"), Some(1));
        let found = paths(&["a/FooBar.cs", "a/BazBar.cs"]);
        assert_eq!(pick(&found, "Bar"), None);
        assert_eq!(pick(&paths(&["a/FooBar.cs"]), "Bar"), Some(0));
    }

    #[test]
    fn a_range_past_the_end_is_not_a_range() {
        assert_eq!(range("63-64", 42), None);
        assert_eq!(range("40-43", 42), None);
        assert_eq!(range("5-3", 42), None);
        assert_eq!(range("0-3", 42), None);
        assert_eq!(range("x", 42), None);
        assert_eq!(range("40-42", 42), Some((40, 42)));
        assert_eq!(range("7", 42), Some((7, 7)));
        assert_eq!(range("", 42), Some((1, 42)));
    }
}
