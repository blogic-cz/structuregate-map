//! IS THIS A PATH, AND WHERE DOES IT LEAD. A doc names paths in code spans, link targets and command
//! lines, and most of what sits in those is not a path at all: a flag, a symbol (`MapGraph.Resolve`),
//! a placeholder (`<tree>\buildmap.json`), a glob, a number. So the question is asked in two steps -
//! does the text CLAIM a file, and does the file exist where a reader would look - and a text that
//! claims nothing is dropped rather than reported, because a report over every symbol would bury the
//! one path that is gone.

use std::path::{Component, Path, PathBuf};

/// A bare name with one of these endings is a FILE even without a folder in it (`buildmap.json`).
/// Without the list `config.X`, `json.dump` and `files.lang` would each be a missing file.
const EXTENSIONS: [&str; 52] = [
    "md", "cs", "csproj", "props", "targets", "sln", "slnx", "rs", "toml", "py", "ps1", "psm1", "psd1", "ts",
    "tsx", "mts", "cts", "js", "mjs", "cjs", "jsx", "json", "jsonc", "yml", "yaml", "sql", "sqlproj", "txt",
    "html", "htm", "css", "scss", "xml", "config", "sh", "cmd", "bat", "exe", "dll", "lib", "sqlite", "db",
    "lock", "ini", "razor", "cshtml", "gs", "spec", "cfg", "csv", "svg", "png",
];

/// Characters no path in these trees carries, and every placeholder, glob, variable and call does.
const NOT_IN_A_PATH: [char; 19] =
    ['<', '>', '*', '?', '{', '}', '$', '%', '|', '"', '\'', '`', '=', '(', ')', '[', ']', '@', ','];

/// The path a text claims, normalised to `/`, or `None` when it claims none.
pub fn claimed(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() || text.len() > 260 || text.contains(char::is_whitespace) {
        return None;
    }
    if text.contains("://") || text.starts_with("mailto:") || text.starts_with('-') || text.starts_with('#') {
        return None;
    }
    if text.contains(&NOT_IN_A_PATH[..]) {
        return None;
    }
    let mut path = text.replace('\\', "/");
    // A LOCATION IS STILL A PATH: `Map.cs:198`, `Map.cs:198:12`, `x.py::Class.method`, `doc.md#part`.
    if let Some(at) = path.find('#') {
        path.truncate(at);
    }
    if let Some(at) = path.find("::") {
        path.truncate(at);
    }
    while let Some(at) = path.rfind(':') {
        let tail = &path[at + 1..];
        // `:42`, `:42-50` - and `:name` / `:Class.method` after a FILE: `checks.py:run_checks`. Never a
        // drive (`C:/x`): what follows a drive's colon is a slash.
        let line = !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit() || c == '-');
        let symbol = tail.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            && tail.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
            && names_a_file(&path[..at]);
        if !line && !symbol {
            break;
        }
        path.truncate(at);
    }
    // A TRAILING `/` SAYS FOLDER, in so many words: `agents/` is the folder a doc points a reader at, where `agents`
    // alone is a word. It is what lets a folder of docs be reached by naming the folder (`graph/reach.rs`).
    let folder = path.ends_with('/');
    let path = path.trim_start_matches("./").trim_end_matches('/').to_string();
    if path.is_empty() || path == "." || path == ".." {
        return None;
    }
    // `/S`, `/nologo`: a Windows switch, not a folder at the root of the drive.
    if path.starts_with('/') && !path[1..].contains('/') && !path.contains('.') {
        return None;
    }
    if path.contains('/') || folder || names_a_file(&path) { Some(path) } else { None }
}

/// A path through a `bin/` or `obj/` folder is a BUILD OUTPUT - `bin\Debug\App.Tests.dll` is there after a
/// build and nowhere before one, so its absence says nothing about the doc.
pub fn built(path: &str) -> bool {
    path.split('/').any(|part| part.eq_ignore_ascii_case("bin") || part.eq_ignore_ascii_case("obj"))
}

/// Whether the LAST segment is a file name - a known extension, or a dotfile (`.gitignore`). A folder
/// path without one is as often an API route (`api/v1/login`) as a directory.
pub fn names_a_file(path: &str) -> bool {
    let last = path.rsplit('/').next().unwrap_or(path);
    last.rsplit_once('.').is_some_and(|(stem, ext)| {
        EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()) && !stem.is_empty() || stem.is_empty() && !ext.is_empty()
    })
}

/// Where a claimed path leads: beside the doc first, then in each folder ABOVE it up to the root - and
/// under the folder a `cd` moved to when there was one, since that is where the reader stands. The
/// folders above count because a nested CLAUDE.md names paths from its project's root or from `src/`:
/// `App.DB/Sales/Tables/X.sql` in `src/A/B/CLAUDE.md` is `src/App.DB/...`. The
/// answer is relative to the root with `/`, or absolute when it lies outside the root. `None` when it
/// is nowhere on that walk - a path under the doc's own subtree is the C# side's to find (`MdMap`).
pub fn locate(root: &Path, doc_rel: &str, path: &str, cd: Option<&str>) -> Option<String> {
    let claimed = Path::new(path);
    if claimed.is_absolute() || path.as_bytes().get(1) == Some(&b':') {
        return claimed.exists().then(|| path.to_string());
    }
    let beside = root.join(doc_rel).parent().map(Path::to_path_buf).unwrap_or_else(|| root.to_path_buf());
    let mut bases = Vec::new();
    if let Some(folder) = cd.and_then(claimed_folder) {
        bases.push(beside.join(&folder));
        bases.push(root.join(&folder));
    }
    let top = lexical(root);
    let mut folder = Some(lexical(&beside));
    while let Some(here) = folder {
        let stop = here == top || !here.starts_with(&top);
        folder = if stop { None } else { here.parent().map(Path::to_path_buf) };
        bases.push(here);
    }
    if bases.last().map(|last| lexical(last)) != Some(top) {
        bases.push(root.to_path_buf());
    }
    let found = bases.into_iter().map(|base| lexical(&base.join(path))).find(|candidate| candidate.exists())?;
    Some(match found.strip_prefix(lexical(root)) {
        Ok(inside) => inside.to_string_lossy().replace('\\', "/"),
        Err(_) => found.to_string_lossy().replace('\\', "/"),
    })
}

/// A path from another folder a doc may write it from - a code root, the repository's top - as an ABSOLUTE
/// path when it is there: the caller keys it by the root it lands in.
pub fn from(base: &Path, path: &str) -> Option<String> {
    if Path::new(path).is_absolute() || path.as_bytes().get(1) == Some(&b':') || !path.contains('/') {
        return None;
    }
    let found = lexical(&base.join(path));
    found.exists().then(|| found.to_string_lossy().replace('\\', "/"))
}

/// EVERY FILE AND FOLDER THE WALK SAW under one root, for the last step of `locate`: a doc's own subtree.
/// A CLAUDE.md describes its folder and names paths from the project inside it -
/// `Infrastructure/Auth/Jwt.cs` in `Shop/CLAUDE.md` is `Shop/src/Shop.Domain/Infrastructure/Auth/Jwt.cs`
/// - and walking UP from the doc never reaches that. Compared ignoring ASCII case, as the file system does.
pub struct Tree {
    paths: Vec<(String, String)>,
}

impl Tree {
    pub fn new(files: &[String]) -> Tree {
        let mut all = std::collections::BTreeSet::new();
        for rel in files {
            let rel = rel.replace('\\', "/");
            let mut at = rel.len();
            while let Some(slash) = rel[..at].rfind('/') {
                all.insert(rel[..slash].to_string());
                at = slash;
            }
            all.insert(rel);
        }
        Tree { paths: all.into_iter().map(|p| (p.to_ascii_lowercase(), p)).collect() }
    }

    /// THE ONE PATH IN THE WHOLE TREE that ends in the claimed one, or none when several do. Under `--doc-root` a doc
    /// names a file from a code root (`core/registry/store.py`) that neither its own folder nor the doc root
    /// reaches; a path that is unique in the tree names that file, and an ambiguous one is not guessed.
    pub fn unique(&self, path: &str) -> Option<String> {
        if Path::new(path).is_absolute() || path.as_bytes().get(1) == Some(&b':') || path.starts_with("..") || !path.contains('/') {
            return None;
        }
        let tail = format!("/{}", path.trim_start_matches('/').to_ascii_lowercase());
        let mut found = self.paths.iter().filter(|(lower, _)| format!("/{lower}").ends_with(&tail));
        let first = found.next()?;
        found.next().is_none().then(|| first.1.clone())
    }

    /// The first path under the doc's folder that ENDS in the claimed one, segment for segment.
    pub fn under(&self, doc_rel: &str, path: &str) -> Option<String> {
        if Path::new(path).is_absolute() || path.as_bytes().get(1) == Some(&b':') || path.starts_with("..") {
            return None;
        }
        let folder = doc_rel.rfind('/').map_or(String::new(), |at| doc_rel[..=at].to_ascii_lowercase());
        let tail = format!("/{}", path.trim_start_matches('/').to_ascii_lowercase());
        self.paths
            .iter()
            .find(|(lower, _)| lower.starts_with(&folder) && format!("/{lower}").ends_with(&tail))
            .map(|(_, original)| original.clone())
    }
}

fn claimed_folder(cd: &str) -> Option<String> {
    let folder = cd.replace('\\', "/");
    (!folder.contains(&NOT_IN_A_PATH[..])).then_some(folder)
}

/// `a/b/../c` as `a/c`, without asking the disk: a path through a junction must keep its own name,
/// and `canonicalize` would answer with the target's.
fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_claimed_and_a_symbol_or_a_placeholder_is_not() {
        assert_eq!(claimed("src/Map/Map.cs:198").as_deref(), Some("src/Map/Map.cs"));
        assert_eq!(claimed("buildmap.json").as_deref(), Some("buildmap.json"));
        assert_eq!(claimed(".claude/skills/").as_deref(), Some(".claude/skills"));
        assert_eq!(claimed("x.py::Class.method").as_deref(), Some("x.py"));
        assert_eq!(claimed(".gitignore").as_deref(), Some(".gitignore"));
        // A trailing slash makes a single word a folder; without it the word is only a word.
        assert_eq!(claimed("agents/").as_deref(), Some("agents"));
        assert_eq!(claimed("pkg/rules/checks.py:run_checks").as_deref(), Some("pkg/rules/checks.py"));
        assert_eq!(claimed("core/store.py:Store.get").as_deref(), Some("core/store.py"));
        assert_eq!(claimed("pkg/pipeline/steps.py::_run_step").as_deref(), Some("pkg/pipeline/steps.py"));
        assert_eq!(claimed("agents"), None);
        for not in ["MapGraph.Resolve", "json.dump", "<tree>\\buildmap.json", "--map", "tests\\**\\*.ps1",
                    "0.21", "$G", "/S", "setup(entry_points=...)", "https://x.io/a.md", "@acme/core"] {
            assert_eq!(claimed(not), None, "{not}");
        }
    }

    #[test]
    fn a_path_is_found_under_the_docs_own_folder_by_its_whole_segments_and_never_in_a_sibling() {
        let tree = Tree::new(&["Shop/src/Shop.Domain/Infrastructure/Auth/Jwt.cs".to_string(),
                               "Other/Shared/Only.cs".to_string(), "Shop/src/MyAuth/Jwt.cs".to_string()]);
        assert_eq!(tree.under("Shop/CLAUDE.md", "Infrastructure/Auth/Jwt.cs").as_deref(),
                   Some("Shop/src/Shop.Domain/Infrastructure/Auth/Jwt.cs"));
        assert_eq!(tree.under("Shop/CLAUDE.md", "infrastructure/auth").as_deref(),
                   Some("Shop/src/Shop.Domain/Infrastructure/Auth"), "a folder, and case-blind");
        assert_eq!(tree.under("Shop/CLAUDE.md", "Shared/Only.cs"), None, "a sibling is not under the doc");
        assert_eq!(tree.under("Shop/CLAUDE.md", "Auth/Jwt.cs").as_deref(),
                   Some("Shop/src/Shop.Domain/Infrastructure/Auth/Jwt.cs"), "`MyAuth/` is not `Auth/`");
        assert_eq!(tree.under("CLAUDE.md", "Shared/Only.cs").as_deref(), Some("Other/Shared/Only.cs"), "the root owns all");
    }

    #[test]
    fn a_path_resolves_beside_the_doc_then_at_the_root_and_never_outside_by_accident() {
        let root = std::env::temp_dir().join("fbt-mdmap-names");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::create_dir_all(root.join("src/app")).unwrap();
        std::fs::write(root.join("docs/near.md"), "").unwrap();
        std::fs::write(root.join("top.json"), "").unwrap();
        std::fs::write(root.join("src/app/run.py"), "").unwrap();
        assert_eq!(locate(&root, "docs/a.md", "near.md", None).as_deref(), Some("docs/near.md"));
        assert_eq!(locate(&root, "docs/a.md", "top.json", None).as_deref(), Some("top.json"));
        assert_eq!(locate(&root, "docs/a.md", "run.py", Some("src/app")).as_deref(), Some("src/app/run.py"));
        assert_eq!(locate(&root, "docs/a.md", "gone.py", None), None);
        // From a folder ABOVE the doc: `app/run.py` named in `src/app/deep/x.md` is `src/app/run.py`.
        std::fs::create_dir_all(root.join("src/app/deep")).unwrap();
        assert_eq!(locate(&root, "src/app/deep/x.md", "app/run.py", None).as_deref(), Some("src/app/run.py"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
