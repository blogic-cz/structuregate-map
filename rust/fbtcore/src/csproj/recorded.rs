//! WHAT THE BUILD ITSELF SAID IT COMPILED - `obj/<config>/<tfm>/structuregate.inputs.tsv`, written after every
//! `CoreCompile` by `StructureGate.targets` wherever it is imported. The build's own references, `#if`
//! symbols, language version and file list, so the compilation this map builds is the one the build built,
//! instead of the reconstruction `project.rs` works out from a restore's leftovers.
//!
//! A RECORD OLDER THAN THE PROJECT FILE IS NOT USED: an edit to the `.csproj` since the last build may have
//! changed any of it, and the reconstruction is the honest answer until the next build writes a new one.

use super::disk;

pub struct Recorded {
    pub lang: String,
    pub defines: Vec<String>,
    pub references: Vec<String>,
    /// Every `Compile` item, as the build listed it - authored and generated alike.
    pub compile: Vec<String>,
}

/// The newest record for this framework, when it is newer than the project file.
pub fn find(csproj: &str, folder: &str, framework: &str) -> Option<Recorded> {
    let obj = disk::combine(&[folder, "obj"]);
    if !disk::is_dir(&obj) {
        return None;
    }
    let candidates: Vec<String> = disk::named_below(&obj, framework)?.into_iter()
        .map(|f| disk::combine(&[&f, "structuregate.inputs.tsv"]))
        .filter(|f| disk::is_file(f))
        .collect();
    let modified = |path: &str| std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let newest = candidates.into_iter().max_by_key(|f| modified(f))?;
    if modified(&newest)? < modified(csproj)? {
        return None;
    }
    let text = disk::text(&newest)?;
    let mut record = Recorded { lang: String::new(), defines: Vec::new(), references: Vec::new(), compile: Vec::new() };
    let mut named = false;
    for line in text.lines() {
        let Some((kind, value)) = line.split_once('\t') else { continue };
        let value = value.trim();
        match kind {
            "tfm" => named = value.eq_ignore_ascii_case(framework),
            "lang" => record.lang = value.to_string(),
            "define" => record.defines = value.split(';').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect(),
            "ref" if !value.is_empty() => record.references.push(value.to_string()),
            "compile" if !value.is_empty() => record.compile.push(value.to_string()),
            _ => {}
        }
    }
    // A record that does not say which framework it was built for, or says another, is not this one's.
    named.then_some(record)
}
