//! WHAT THE PROJECT READERS SHARE: paths joined and made absolute the way .NET joins them (every path here ends
//! up in a Roslyn compilation, and two spellings of one file are two references), folders listed in the order
//! the file system returns them, and XML read by LOCAL name - a `.csproj` may carry the MSBuild namespace.

use std::path::{Path, MAIN_SEPARATOR};

/// `Path.Combine`: a rooted part starts again, a separator is added only when the left side has none.
pub fn combine(parts: &[&str]) -> String {
    let mut out = String::new();
    for part in parts {
        if part.is_empty() {
            continue;
        }
        let part = separators(part);
        if rooted(&part) || out.is_empty() {
            out = part.into_owned();
            continue;
        }
        if !out.ends_with(['\\', '/']) {
            out.push(MAIN_SEPARATOR);
        }
        out.push_str(&part);
    }
    out
}

/// A path as MSBuild reads it: `..\lib\Lib.csproj` is written with backslashes on every OS, and MSBuild takes
/// a backslash for a separator everywhere. Off Windows the file system does not, so it is turned here.
fn separators(part: &str) -> std::borrow::Cow<'_, str> {
    if cfg!(windows) || !part.contains('\\') {
        return std::borrow::Cow::Borrowed(part);
    }
    std::borrow::Cow::Owned(part.replace('\\', "/"))
}

/// `Path.IsPathRooted`: a drive, a leading separator, or a UNC path.
pub fn rooted(path: &str) -> bool {
    path.starts_with(['\\', '/']) || path.as_bytes().get(1) == Some(&b':')
}

/// `Path.GetFullPath`: absolute, `.` and `..` resolved. `std::path::absolute` resolves them on Windows only -
/// on Unix it keeps `..`, which makes `app/../lib/Lib.csproj` and `lib/Lib.csproj` two projects - so off
/// Windows they are folded here, by name, the way .NET does it (never through a symlink).
pub fn full(path: &str) -> String {
    let Ok(absolute) = std::path::absolute(&*separators(path)) else { return path.to_string() };
    if cfg!(windows) {
        return absolute.to_string_lossy().into_owned();
    }
    let mut folded = std::path::PathBuf::new();
    for part in absolute.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                folded.pop();
            }
            other => folded.push(other),
        }
    }
    folded.to_string_lossy().into_owned()
}

/// `Path.GetDirectoryName`.
pub fn parent(path: &str) -> Option<String> {
    Path::new(path).parent().map(|p| p.to_string_lossy().into_owned()).filter(|p| !p.is_empty())
}

pub fn file_name(path: &str) -> String {
    Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

pub fn stem(path: &str) -> String {
    Path::new(path).file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

pub fn is_file(path: &str) -> bool {
    Path::new(path).is_file()
}

pub fn is_dir(path: &str) -> bool {
    Path::new(path).is_dir()
}

/// The files of a folder with an extension (`.dll`), in the order the file system lists them; None when the
/// folder cannot be read.
pub fn files(folder: &str, extension: &str) -> Option<Vec<String>> {
    let entries = std::fs::read_dir(folder).ok()?;
    Some(entries.flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.len() > extension.len() && name[name.len() - extension.len()..].eq_ignore_ascii_case(extension))
        .map(|name| combine(&[folder, &name]))
        .collect())
}

/// Every file with the extension at any depth below `root`, breadth first, never inside a folder named `skip`.
pub fn files_below(root: &str, extension: &str, skip: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = std::collections::VecDeque::from([root.to_string()]);
    while let Some(folder) = pending.pop_front() {
        found.extend(files(&folder, extension).unwrap_or_default());
        for child in folders(&folder).unwrap_or_default() {
            if !file_name(&child).eq_ignore_ascii_case(skip) {
                pending.push_back(child);
            }
        }
    }
    found
}

/// The sub-folders of a folder, in the order the file system lists them.
pub fn folders(folder: &str) -> Option<Vec<String>> {
    let entries = std::fs::read_dir(folder).ok()?;
    Some(entries.flatten().filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| combine(&[folder, &e.file_name().to_string_lossy()])).collect())
}

/// Every folder under `root` named `name`, searched breadth first as .NET searches `AllDirectories`.
pub fn named_below(root: &str, name: &str) -> Option<Vec<String>> {
    let mut found = Vec::new();
    let mut pending = std::collections::VecDeque::from([root.to_string()]);
    let mut first = true;
    while let Some(folder) = pending.pop_front() {
        let Some(children) = folders(&folder) else {
            if first {
                return None;
            }
            continue;
        };
        first = false;
        for child in children {
            if file_name(&child).eq_ignore_ascii_case(name) {
                found.push(child.clone());
            }
            pending.push_back(child);
        }
    }
    Some(found)
}

/// The newest (by last write) of some folders; the first listed wins a tie.
pub fn newest(folders: Vec<String>) -> Option<String> {
    let stamped: Vec<(std::time::SystemTime, String)> = folders.into_iter()
        .map(|f| (std::fs::metadata(&f).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), f))
        .collect();
    let best = stamped.iter().map(|(t, _)| *t).max()?;
    stamped.into_iter().find(|(t, _)| *t == best).map(|(_, f)| f)
}

/// A file's text as XML reads it: a byte-order mark honoured, UTF-16 decoded.
pub fn text(path: &str) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    if let Some(rest) = bytes.strip_prefix(b"\xEF\xBB\xBF") {
        return Some(String::from_utf8_lossy(rest).into_owned());
    }
    if let Some(rest) = bytes.strip_prefix(b"\xFF\xFE") {
        let units: Vec<u16> = rest.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        return Some(String::from_utf16_lossy(&units));
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

pub fn parse(text: &str) -> Option<roxmltree::Document<'_>> {
    let options = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    roxmltree::Document::parse_with_options(text, options).ok()
}

/// Every element under the root with this LOCAL name, in document order - `Descendants()`.
pub fn elements<'a, 'i>(document: &'a roxmltree::Document<'i>, name: &'a str) -> impl Iterator<Item = roxmltree::Node<'a, 'i>> + 'a {
    document.root_element().descendants().skip(1).filter(move |n| n.is_element() && n.tag_name().name() == name)
}

/// An element's text, every text node under it joined - `XElement.Value`.
pub fn value(node: roxmltree::Node) -> String {
    node.descendants().filter(|n| n.is_text()).filter_map(|n| n.text()).collect()
}

/// The first non-empty trimmed value of an element named `name` - a PROPERTY as this pass reads one.
pub fn property(document: &roxmltree::Document, name: &str) -> Option<String> {
    elements(document, name).map(|e| value(e).trim().to_string()).find(|v| !v.is_empty())
}

/// The first child element with this local name.
pub fn child<'a, 'i>(node: roxmltree::Node<'a, 'i>, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
    node.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

/// A version folder's name as numbers split on `.` and `-`, a part that is not one being 0.
pub fn version_parts(folder: &str) -> Vec<i64> {
    file_name(folder).split(['.', '-']).map(|p| p.parse::<i64>().unwrap_or(0)).collect()
}

/// `DOTNET_ROOT`, else where the platform installs it: `%ProgramFiles%\dotnet` on Windows.
pub fn dotnet(require_existing: bool) -> String {
    match std::env::var("DOTNET_ROOT") {
        Ok(root) if !root.is_empty() && (!require_existing || is_dir(&root)) => root,
        _ => installed_dotnet(),
    }
}

#[cfg(windows)]
fn installed_dotnet() -> String {
    combine(&[&std::env::var("ProgramFiles").unwrap_or_default(), "dotnet"])
}

/// Off Windows there is no one place: a package manager, the install script (`~/.dotnet`) and Homebrew each
/// pick their own. The `dotnet` on PATH is the one a build ran, so its folder - through any symlink - is the
/// answer; the package managers' folders are the fallback when PATH has none.
#[cfg(not(windows))]
fn installed_dotnet() -> String {
    let on_path = std::env::var_os("PATH").into_iter()
        .flat_map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .map(|folder| folder.join("dotnet"))
        .find(|candidate| candidate.is_file())
        .and_then(|candidate| std::fs::canonicalize(candidate).ok())
        .and_then(|real| real.parent().map(|folder| folder.to_string_lossy().into_owned()));
    on_path
        .or_else(|| ["/usr/share/dotnet", "/usr/lib/dotnet", "/usr/local/share/dotnet"].into_iter().find(|f| is_dir(f)).map(String::from))
        .unwrap_or_else(|| "/usr/share/dotnet".into())
}

/// `NUGET_PACKAGES`, else `.nuget/packages` in the user's home - `%USERPROFILE%` on Windows, `$HOME` elsewhere.
pub fn packages() -> String {
    let home = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    match std::env::var("NUGET_PACKAGES") {
        Ok(folder) if !folder.is_empty() => folder,
        _ => combine(&[&std::env::var(home).unwrap_or_default(), ".nuget", "packages"]),
    }
}
