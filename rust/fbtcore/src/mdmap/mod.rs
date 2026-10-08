//! THE MARKDOWN HALF - what each doc names, and whether it is there, parsed by `pulldown-cmark`
//! inside this process. Markdown has no host of its own the way python or TypeScript do, so its
//! parser lives in the exe like `syn` does for rust.
//!
//! A DOC IS NOT CODE, and the answer is shaped so the map cannot mistake it for code: a doc declares
//! nothing and uses no name, so it never becomes an importer of a source file and never hides one from
//! `NO READER`. What it has instead is MENTIONS - the files it points a reader at - each resolved the
//! way a reader would follow it, beside the doc and then at the root. A mention that resolves is an
//! edge (`mentioned_by` is the "which docs go stale if I move this" query); one that does not is a doc
//! sending every session that reads it to a file that is not there.

pub(crate) mod names;
mod read;

use serde_json::{json, Value};
use std::path::Path;



/// `anywhere`: under `--doc-root`, a path unique in the whole tree resolves to that file (`Tree::unique`); `also` are
/// the folders tried last - every code root, and the git top level.
pub(crate) fn map_file(root: &Path, rel: &str, abs: &Path, tree: &names::Tree, anywhere: bool, also: &[std::path::PathBuf]) -> Value {
    match std::fs::read_to_string(abs) {
        Ok(text) => row(root, rel, text.trim_start_matches('\u{feff}'), tree, anywhere, also),
        Err(e) => json!({ "rel": rel, "error": { "line": 1, "message": format!("could not be read ({e})") } }),
    }
}

fn row(root: &Path, rel: &str, text: &str, tree: &names::Tree, anywhere: bool, also: &[std::path::PathBuf]) -> Value {
    let doc = read::walk(text);
    let mut mentions = Vec::new();
    let mut missing = Vec::new();
    for named in &doc.named {
        let Some(path) = names::claimed(&named.text) else { continue };
        // Up from the doc first, then DOWN into its own subtree - for a path with a folder only: a bare name
        // has too many candidates under a big folder to say which one it meant.
        let found = names::locate(root, rel, &path, named.cd.as_deref())
            .or_else(|| path.contains('/').then(|| tree.under(rel, &path)).flatten())
            .or_else(|| anywhere.then(|| tree.unique(&path)).flatten())
            .or_else(|| {
                // ONE FOLDER ONLY: `src/lib.rs` under two code roots names neither - it is not guessed.
                let mut hits: Vec<String> = also.iter().filter_map(|base| names::from(base, &path)).collect();
                hits.sort();
                hits.dedup();
                (hits.len() == 1).then(|| hits.remove(0))
            });
        match found {
            Some(found) => mentions.push(json!({ "path": found, "line": named.line, "kind": named.kind })),
            // A BARE NAME IS NOT A LOCATION. `Map.cs` names a file without saying where it is, so it is an
            // edge when it lies beside the doc or at the root and nothing when it does not; only a path
            // WITH a folder claims a place a reader will go looking. A LINK always does: `](notes.md)` is
            // "beside this doc", and following it is what a link is for. In a CODE SPAN a folder path must
            // also end in a file name - `api/v1/login` and `/auth/token` are routes, not places on disk;
            // a word of a shell block is run, so a folder there (`cd src/app`) is a place.
            // A BUILD OUTPUT (`bin/`, `obj/`) is absent until something is built, so it is never missing.
            None if !names::built(&path)
                && (named.kind == "link"
                    || path.contains('/') && (named.kind == "command" || names::names_a_file(&path))) =>
            {
                missing.push(json!({ "text": named.text, "line": named.line, "kind": named.kind }));
            }
            None => {}
        }
    }
    // WHAT A READER SEES FIRST: a skill or an agent is known by its frontmatter, every other doc by
    // its first heading.
    let summary = match (&doc.name, &doc.description) {
        (Some(name), Some(description)) => format!("{name}: {}", first_sentence(description)),
        (Some(name), None) => name.clone(),
        _ => doc.title.clone(),
    };
    json!({ "rel": rel, "summary": summary, "name": doc.name, "mentions": mentions, "missing": missing })
}

fn first_sentence(text: &str) -> &str {
    let end = text.find(". ").map_or(text.len(), |at| at + 1);
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    const SKILL: &str = "---\nname: demo\ndescription: \"Ask it things. Then more: with a colon.\"\n---\n\
        # Demo skill\n\nRead `present.md` and `docs/gone.json`, not `Map.Resolve`, `nowhere.cs`, `/S` or the route `api/v1/login`.\n\n\
        ```bash\ncd src/app\npython run.py --check   # the hook\n$env:PATH = \"C:\\Program Files (x86)\\x;$env:PATH\"\n```\n\n\
        ```json\n{ \"x\": \"never.json\" }\n```\n\nSee [the notes](notes.md).\n";

    #[test]
    fn a_doc_mentions_what_is_there_and_misses_what_is_not_with_its_line() {
        let root = std::env::temp_dir().join("fbt-mdmap-row");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src/app")).unwrap();
        std::fs::write(root.join("present.md"), "").unwrap();
        std::fs::write(root.join("src/app/run.py"), "").unwrap();
        let row = row(&root, "SKILL.md", SKILL, &names::Tree::new(&[]), false, &[]);
        assert_eq!(row["name"], "demo");
        assert_eq!(row["summary"], "demo: Ask it things.");
        let paths: Vec<&str> = row["mentions"].as_array().unwrap().iter().map(|m| m["path"].as_str().unwrap()).collect();
        assert_eq!(paths, ["present.md", "src/app", "src/app/run.py"]);
        let gone: Vec<(&str, u64)> = row["missing"].as_array().unwrap().iter()
            .map(|m| (m["text"].as_str().unwrap(), m["line"].as_u64().unwrap())).collect();
        // `never.json` sits in a JSON block - example content, never a claim. `nowhere.cs` is a bare name,
        // `/S` a switch, `api/v1/login` a route, and the quoted `C:\Program Files (x86)` one word: none of them is missing.
        assert_eq!(gone, [("docs/gone.json", 7), ("notes.md", 19)]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
