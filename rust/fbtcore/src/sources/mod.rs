//! WHICH FILES - the walk every rule and every map half measures over. Two modes, and the difference is
//! not cosmetic:
//!
//! DISK walks the tree - everything on the filesystem, which is what a build sees.
//! GIT asks `git ls-files`, which is what a COMMIT will contain: build output that escaped the skip list,
//! a stale scratch file and an editor backup all disappear, and a deleted-but-unstaged file is still
//! listed - handed back as `deleted`, for the caller to name if it is one it would have measured.
//!
//! It walks and nothing else: what a file IS (source, doc, which half) and how many lines it has are
//! the callers' questions.

pub(crate) mod glob;

use serde::Deserialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf, MAIN_SEPARATOR};

/// Generated or vendored code that is tracked but not hand-written.
const SKIP_SUFFIXES: [&str; 1] = ["_pb2.py"];
const SKIP_NAME_MARKERS: [&str; 1] = [".generated."];

#[derive(Deserialize, Default)]
#[serde(default)]
pub(crate) struct Input {
    pub root: String,
    /// Folder names never entered - compared ignoring case, as `--skip` always has been.
    pub skip: Vec<String>,
    pub tracked: bool,
    pub include_untracked: bool,
    /// THE MAP LISTS A GENERATED `.cs` rather than losing it: the gate skips `*.Generated.cs` (nobody wrote
    /// it), but the project compiles it, and a deep map without its row reads as a tree without the file.
    pub generated_cs: bool,
}

/// What a walk found: `[abs, rel]` sorted by `rel`, the tracked files deleted in the working tree, and the
/// folders it could not read.
pub(crate) struct Walked {
    pub found: Vec<(String, String)>,
    pub deleted: Vec<String>,
    pub unreadable: Vec<String>,
}


struct Walk {
    skip: BTreeSet<String>,
    generated_cs: bool,
    found: Vec<(String, String)>,
    deleted: Vec<String>,
    unreadable: Vec<String>,
}


pub(crate) fn walked(input: &Input) -> Walked {
    let mut walk = Walk {
        skip: input.skip.iter().map(|s| s.to_lowercase()).collect(),
        generated_cs: input.generated_cs,
        found: Vec::new(),
        deleted: Vec::new(),
        unreadable: Vec::new(),
    };
    let root = PathBuf::from(&input.root);
    if input.tracked {
        from_git(&mut walk, &root, input.include_untracked);
    } else {
        from_disk(&mut walk, &root, &root);
    }
    walk.found.sort_by(|a, b| a.1.cmp(&b.1));
    walk.deleted.sort();
    Walked { found: walk.found, deleted: walk.deleted, unreadable: walk.unreadable }
}

/// THE DISK WALK, and two things a real tree holds that a plain recursion cannot survive. A DIRECTORY
/// LINK - a symlink or a junction, like the `lib64 -> lib` every python venv carries - is NOT followed:
/// its target is either walked under its own name already or outside the root. A DIRECTORY THAT CANNOT BE
/// READ is skipped and NAMED, never swallowed: a silent skip is how a rule stops covering the files it was
/// written for.
fn from_disk(walk: &mut Walk, root: &Path, dir: &Path) {
    let entries = match std::fs::read_dir(dir).and_then(|entries| entries.collect::<Result<Vec<_>, _>>()) {
        Ok(entries) => entries,
        Err(e) => {
            walk.unreadable.push(format!("{} ({})", dir.display(), exception(&e)));
            return;
        }
    };
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        // WHAT THE LISTING ALREADY SAYS: an entry that is no link is a folder or a file by the directory's own record,
        // with no call to the file system. Two calls an entry - `metadata`, then `symlink_metadata` for each folder -
        // were most of the walk of a large tree on Windows. A link still takes the old road.
        let folder = match plain(&entry) {
            Some(is_dir) => is_dir,
            None => std::fs::metadata(&path).is_ok_and(|m| m.is_dir()),
        };
        if folder {
            if walk.skip.contains(&name.to_lowercase()) || (plain(&entry).is_none() && is_link(&path)) {
                continue;
            }
            from_disk(walk, root, &path);
        } else if !suffixed(&name) && !marked(&name, walk.generated_cs) {
            let rel = relative(root, &path);
            walk.found.push((path.to_string_lossy().into_owned(), rel));
        }
    }
}

/// THE GIT LIST, scoped to the root: `<scope>/**` and `<scope>/*`, or `*` when the root is the repo.
fn from_git(walk: &mut Walk, root: &Path, include_untracked: bool) {
    let repo = git_root(root);
    let scope = relative(&repo, root);
    let pathspecs: Vec<String> =
        if scope == "." { vec!["*".into()] } else { vec![format!("{scope}/**"), format!("{scope}/*")] };
    let mut listed = git(&repo, &[&["ls-files"], &pathspecs.iter().map(String::as_str).collect::<Vec<_>>()[..]].concat());
    if include_untracked {
        let others = [&["ls-files", "--others", "--exclude-standard"], &pathspecs.iter().map(String::as_str).collect::<Vec<_>>()[..]].concat();
        listed.extend(git(&repo, &others));
    }
    for rel in listed {
        let abs = native(&repo.join(&rel));
        if skipped(walk, &relative(root, &abs)) {
            continue;
        }
        // A file deleted in the working tree and not yet staged is still TRACKED. Counting it would crash;
        // dropping it silently would shrink the gate's input - so it goes back as `deleted`.
        if !abs.is_file() {
            walk.deleted.push(rel);
            continue;
        }
        walk.found.push((abs.to_string_lossy().into_owned(), rel));
    }
}

fn skipped(walk: &Walk, rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty()).collect();
    if parts.iter().any(|p| walk.skip.contains(&p.to_lowercase())) {
        return true;
    }
    let name = parts.last().copied().unwrap_or(rel);
    suffixed(name) || marked(name, walk.generated_cs)
}

fn suffixed(name: &str) -> bool {
    let lower = name.to_lowercase();
    SKIP_SUFFIXES.iter().any(|s| lower.ends_with(s))
}

fn marked(name: &str, generated_cs: bool) -> bool {
    let lower = name.to_lowercase();
    SKIP_NAME_MARKERS.iter().any(|m| lower.contains(m)) && !(generated_cs && lower.ends_with(".cs"))
}

/// Whether an entry is a folder, read off the directory listing - None when it is a LINK of any kind (on Windows any
/// reparse point, as `is_link` tests) or the listing cannot say, and the caller asks the file system as it always did.
/// On Windows `DirEntry::metadata` is the listing's own record; elsewhere `file_type` is the listing's `d_type`.
fn plain(entry: &std::fs::DirEntry) -> Option<bool> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        let meta = entry.metadata().ok()?;
        (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0).then(|| meta.is_dir())
    }
    #[cfg(not(windows))]
    {
        let kind = entry.file_type().ok()?;
        (!kind.is_symlink()).then(|| kind.is_dir())
    }
}

/// A symlink or junction, by the REPARSE POINT attribute on Windows - any reparse point, as the walk has
/// always tested. On a failure the answer is NO on purpose: the walk then tries the folder, fails there,
/// and names it as unreadable - answering yes would drop it without a word.
fn is_link(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        std::fs::symlink_metadata(path).is_ok_and(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
    }
    #[cfg(not(windows))]
    {
        std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
    }
}

/// What the .NET exception for this error was called - the walk's messages have always named it.
fn exception(e: &std::io::Error) -> &'static str {
    match e.kind() {
        std::io::ErrorKind::PermissionDenied => "UnauthorizedAccessException",
        std::io::ErrorKind::NotFound => "DirectoryNotFoundException",
        _ => "IOException",
    }
}

/// `path` relative to `base` with `/` - "." for the base itself. The prefix is compared IGNORING CASE on
/// Windows: git names the repo root in its own spelling, and `C:\Src` is `c:/src` there.
pub fn relative(base: &Path, path: &Path) -> String {
    let base = native(base).to_string_lossy().trim_end_matches(MAIN_SEPARATOR).to_string();
    let full = native(path).to_string_lossy().into_owned();
    let same = |a: &str, b: &str| if cfg!(windows) { a.eq_ignore_ascii_case(b) } else { a == b };
    if same(&full, &base) {
        return ".".into();
    }
    let cut = base.len() + 1;
    if full.len() > cut && same(&full[..base.len()], &base) && full[base.len()..].starts_with(MAIN_SEPARATOR) {
        return full[cut..].replace(MAIN_SEPARATOR, "/");
    }
    full.replace(MAIN_SEPARATOR, "/")
}

/// The platform's separator throughout - git answers with `/`.
fn native(path: &Path) -> PathBuf {
    if cfg!(windows) { PathBuf::from(path.to_string_lossy().replace('/', "\\")) } else { path.to_path_buf() }
}

/// Ask git, never count parent directories: a pipeline can check a project out at a different depth than
/// the dev tree, and a hardcoded parent lands on the wrong root - where the gate measures nothing.
/// The top of the git repository `start` is in, or None outside one.
pub(crate) fn repository(start: &Path) -> Option<PathBuf> {
    git(start, &["rev-parse", "--show-toplevel"]).into_iter().next().map(|top| native(Path::new(&top)))
}

fn git_root(start: &Path) -> PathBuf {
    git(start, &["rev-parse", "--show-toplevel"]).into_iter().next().map(|top| native(Path::new(&top))).unwrap_or_else(|| start.to_path_buf())
}

/// `core.quotepath=off`, or git prints a non-ASCII path as C-style octal escapes inside quotes -
/// `"docs/Caf\303\251.md"` - which matches nothing on disk, and a non-ASCII file name is silently dropped.
fn git(cwd: &Path, args: &[&str]) -> Vec<String> {
    let Ok(out) = std::process::Command::new("git").current_dir(cwd).arg("-c").arg("core.quotepath=off").args(args).output() else {
        return Vec::new();
    };
    String::from_utf8_lossy(&out.stdout).split('\n').map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_skipped_folder_a_generated_file_and_a_pb2_module_are_not_walked() {
        let root = std::env::temp_dir().join("fbt-sources-walk");
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["src", "Bin", "src/deep"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        for file in ["src/a.cs", "Bin/b.cs", "src/deep/c.Generated.cs", "src/x_pb2.py", "src/deep/d.md"] {
            std::fs::write(root.join(file), "").unwrap();
        }
        let input = Input { root: root.to_string_lossy().into_owned(), skip: vec!["bin".into()], ..Input::default() };
        let got = walked(&input);
        let rels: Vec<&str> = got.found.iter().map(|f| f.1.as_str()).collect();
        assert_eq!(rels, ["src/a.cs", "src/deep/d.md"]);
        let input = Input { generated_cs: true, ..input };
        let rels = walked(&input).found.len();
        assert_eq!(rels, 3, "the map lists a generated .cs");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_path_is_relative_to_its_root_whatever_case_git_spells_the_root_in() {
        if cfg!(windows) {
            assert_eq!(relative(Path::new("C:\\Projects\\Tree"), Path::new("c:/projects/tree/src/a.cs")), "src/a.cs");
            assert_eq!(relative(Path::new("C:\\Projects\\Tree\\"), Path::new("C:\\Projects\\Tree")), ".");
        }
    }
}
