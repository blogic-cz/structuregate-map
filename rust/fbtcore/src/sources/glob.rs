//! A GLOB OVER A FILE'S PATH FROM ITS ROOT, for the two flags that leave single files out: `--map-exclude`
//! (C# files the deep map lists and never walks) and `--skip-file` (files the gate does not measure).
//!
//! A CONSUMER'S CHOICE, NEVER THIS TOOL'S DEFAULT. EF seed migrations are hand-written as far as any marker
//! says - `Migrations/` holds files people edit - and on one data project their `InsertData` bodies
//! were most of its rows and most of its binding time. Whether those rows are worth it is a question
//! about one codebase, so the codebase answers it on its own command line.
//!
//! THE PATTERN IS MATCHED BY HAND, not by a pattern language: `**` is any number of folders, `*` any run of
//! characters inside one segment, `?` one character, and nothing else is special. Case is ignored - these are
//! Windows paths, and `migrations/` is the same folder as `Migrations/`.

/// Whether `rel` (a map path, `/`-separated) matches any of `patterns`.
pub fn excluded(patterns: &[String], rel: &str) -> bool {
    let path: Vec<String> = rel.replace('\\', "/").to_lowercase().split('/').filter(|s| !s.is_empty()).map(String::from).collect();
    patterns.iter().any(|pattern| {
        let wanted: Vec<String> =
            pattern.replace('\\', "/").to_lowercase().split('/').filter(|s| !s.is_empty()).map(String::from).collect();
        walk(&wanted, &path)
    })
}

fn walk(pattern: &[String], path: &[String]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((head, rest)) if head == "**" => (0..=path.len()).any(|skip| walk(rest, &path[skip..])),
        Some((head, rest)) => path.split_first().is_some_and(|(name, left)| segment(head, name) && walk(rest, left)),
    }
}

/// One segment: `*` is any run of characters, `?` exactly one.
fn segment(pattern: &str, name: &str) -> bool {
    let (p, n): (Vec<char>, Vec<char>) = (pattern.chars().collect(), name.chars().collect());
    let (mut i, mut j, mut star, mut mark) = (0, 0, None, 0);
    while j < n.len() {
        if i < p.len() && (p[i] == '?' || p[i] == n[j]) {
            i += 1;
            j += 1;
        } else if i < p.len() && p[i] == '*' {
            star = Some(i);
            i += 1;
            mark = j;
        } else if let Some(at) = star {
            i = at + 1;
            mark += 1;
            j = mark;
        } else {
            return false;
        }
    }
    while i < p.len() && p[i] == '*' {
        i += 1;
    }
    i == p.len()
}

#[cfg(test)]
mod tests {
    use super::excluded;

    fn any(patterns: &[&str], rel: &str) -> bool {
        excluded(&patterns.iter().map(|p| p.to_string()).collect::<Vec<_>>(), rel)
    }

    #[test]
    fn a_double_star_crosses_folders_and_a_single_one_does_not() {
        assert!(any(&["**/Migrations/*.cs"], "src/Data/Infrastructure/Ef/Migrations/2023_Init.cs"));
        assert!(any(&["**/Migrations/*.cs"], "Migrations/2023_Init.cs"));
        assert!(!any(&["**/Migrations/*.cs"], "src/Data/Migrations/Scripts/Legacy/2023_Seed.cs"));
        assert!(any(&["**/Migrations/**/*.cs"], "src/Data/Migrations/Scripts/Legacy/2023_Seed.cs"));
        assert!(!any(&["*/Migrations/*.cs"], "a/b/Migrations/x.cs"));
    }

    #[test]
    fn case_is_ignored_and_a_segment_matches_whole() {
        assert!(any(&["**/migrations/*_seed*.cs"], "Data/Migrations/0001_SeedData.cs"));
        assert!(!any(&["**/Migration/*.cs"], "Data/Migrations/x.cs"));
        assert!(any(&["Data/?igrations/x.cs"], "Data/Migrations/x.cs"));
        assert!(!any(&[], "Data/Migrations/x.cs"));
    }
}
