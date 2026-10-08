//! THE SCRIPTS THE HOSTS RUN, compiled INTO this library - so into the exe - and written to disk when a half
//! needs one. A consumer holds two files; every checker and every map half travels inside the first, and a
//! gate whose second half is missing or stale cannot happen.
//!
//! EACH SET IS STAGED INTO ONE FOLDER, because an entry module imports its siblings by a flat relative path
//! (`./TsNodes.mjs`) whatever source subfolder they live in. The folder is named after the HASH of the set, so
//! a rebuilt exe writes a new folder instead of running last week's half out of a stale temp file, and two
//! builds of the same scripts share one.
//!
//! ADDING A SCRIPT IS ONE LINE HERE: `include_bytes!` makes the build fail on a name that is not there, where a
//! resource list only failed at run time, in a consumer, as an `ERR_MODULE_NOT_FOUND`.

use std::path::Path;

/// One set: the folder tag and its files as `(source path under src/, bytes)` - the entry script FIRST.
pub struct Set {
    pub tag: &'static str,
    pub files: &'static [(&'static str, &'static [u8])],
}

macro_rules! set {
    ($tag:literal: $($path:literal),+ $(,)?) => {
        Set {
            tag: $tag,
            files: &[$(($path, include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../src/", $path)) as &[u8])),+],
        }
    };
}

/// The PowerShell checker: the entry script and the files it dot-sources.
pub const PSGATE: Set = set!("psgate": "PsGate/PsGate.ps1", "PsGate/PsGate.Ast.ps1", "PsGate/PsGate.Rules.ps1",
    "PsGate/PsGate.Rules.Scope.ps1", "PsGate/PsGate.Rules.WinForms.ps1");

/// The TypeScript checker. The MAP module is among them because the entry imports it unconditionally.
pub const TSGATE: Set = set!("tsgate": "TsGate/TsGate.mjs", "TsGate/TsGate.Rules.mjs", "TsGate/TsGate.Map.mjs", "TsGate/TsGate.Groups.mjs");

/// The file-level map halves, by the language each answers for.
pub const MAP_TYPESCRIPT: Set = set!("typescript": "TsGate/TsGate.mjs", "TsGate/TsGate.Rules.mjs", "TsGate/TsGate.Map.mjs", "TsGate/TsGate.Groups.mjs");
pub const MAP_PYTHON: Set = set!("python": "Map/Py/PyMap.py", "Map/Py/PyAst.py", "Map/Py/PyBind.py", "Map/Py/PyDeps.py",
    "Map/Py/PyLaunch.py", "Map/Py/PyInputs.py");
pub const MAP_POWERSHELL: Set = set!("powershell": "Map/PsMap.ps1");

/// The Angular half. `TsMap.mjs` imports every other one, so a name missing here fails the BUILD.
pub const TSROWS: Set = set!("tsrows-node":
    "TsRows/TsMap.mjs", "TsRows/TsSetup/TsConfig.mjs", "TsRows/TsImports.mjs", "TsRows/TsInventory.mjs",
    "TsRows/TsLocales.mjs", "TsRows/TsPaths.mjs", "TsRows/TsNodes.mjs", "TsRows/TsSetup/TsProgram.mjs", "TsRows/TsSetup/TsReads.mjs", "TsRows/TsSetup/TsShape.mjs",
    "TsRows/TsSetup/TsProjects.mjs", "TsRows/TsSetup/TsResolve.mjs", "TsRows/TsSetup/TsRunIo.mjs", "TsRows/TsStore.mjs",
    "TsRows/TsText.mjs", "TsRows/TsPlain/TsLiterals.mjs", "TsRows/TsDecls/TsWalk.mjs",
    "TsRows/TsHtml.mjs",
    "TsRows/TsDecls/TsAngular.mjs", "TsRows/TsDecls/TsBody.mjs", "TsRows/TsDecls/TsCatch.mjs",
    "TsRows/TsDecls/TsClassRows.mjs",
    "TsRows/TsDecls/TsDeclRows.mjs", "TsRows/TsDecls/TsExpr/TsExprDescribe.mjs", "TsRows/TsDecls/TsExpr/TsExprNorm.mjs",
    "TsRows/TsDecls/TsExpr/TsExprSummary.mjs", "TsRows/TsDecls/TsExpr/TsReadRefs.mjs", "TsRows/TsDecls/TsI18nCalls.mjs",
    "TsRows/TsDecls/TsModifiers.mjs", "TsRows/TsDecls/TsRefs.mjs", "TsRows/TsDecls/TsTypeRef.mjs",
    "TsRows/TsDecls/TsTypeRows.mjs", "TsRows/TsDecls/TsExpr/TsValue.mjs", "TsRows/TsDecls/TsExpr/TsValueRefs.mjs",
    "TsRows/TsDecls/TsExpr/TsWriteRefs.mjs",
    "TsRows/TsTpl/TsGateChain.mjs", "TsRows/TsTpl/TsRegistry.mjs", "TsRows/TsDerive/TsDynRender.mjs",
    "TsRows/TsDerive/TsIndexes.mjs", "TsRows/TsDerive/TsInputUsage.mjs", "TsRows/TsDerive/TsModuleScope.mjs",
    "TsRows/TsDerive/TsNgrx.mjs", "TsRows/TsDerive/TsReach.mjs", "TsRows/TsDerive/TsRouteFold.mjs",
    "TsRows/TsDerive/TsRoutes.mjs", "TsRows/TsDerive/TsRouteTree.mjs", "TsRows/TsDerive/TsSpecAnchors.mjs",
    "TsRows/TsDerive/TsSpecJoins.mjs", "TsRows/TsTpl/TsTplEmit.mjs", "TsRows/TsTpl/TsTplExpr.mjs",
    "TsRows/TsTpl/TsTplExtract.mjs", "TsRows/TsTpl/TsTplKeys.mjs", "TsRows/TsTpl/TsTplMatch.mjs",
    "TsRows/TsTpl/TsTplReads.mjs");

/// The plain TypeScript half. `TsGate.Map.mjs` is the file map's: sharing it keeps its fingerprints the ones
/// buildmap.json groups by.
pub const TSPLAIN: Set = set!("plain-ts rows": "TsRows/TsPlain/TsPlain.mjs", "TsRows/TsPlain/TsPlainRows.mjs",
    "TsRows/TsPlain/TsPlainFacts.mjs", "TsGate/TsGate.Map.mjs", "TsRows/TsSetup/TsResolve.mjs",
    "TsRows/TsDecls/TsCatch.mjs", "TsRows/TsPlain/TsLiterals.mjs");

/// The python rows half.
pub const PYROWS: Set = set!("python rows": "Map/Py/PyRows.py", "Map/Py/PyAst.py", "Map/Py/PyBind.py",
    "Map/Py/PyDeps.py", "Map/Py/PyLaunch.py", "Map/Py/PyArgs.py", "Map/Py/PyInputs.py", "Map/Py/PyStmts.py", "Map/Py/PyComments.py",
    "Map/Py/PyRegex.py", "Map/Py/PyLiterals.py", "Map/Py/PyKeys.py");

/// Every set a map run may launch, by the name its half asks for; the deep ones only for `--map-sqlite`.
pub fn for_map(deep: bool) -> Vec<&'static Set> {
    let mut sets = vec![&MAP_TYPESCRIPT, &MAP_PYTHON, &MAP_POWERSHELL];
    if deep {
        sets.extend([&TSROWS, &TSPLAIN, &PYROWS]);
    }
    sets
}

/// What a set is, as a hash of its bytes: a half whose scripts changed must run again.
pub fn digest(set: &Set) -> String {
    let mut hasher = blake3::Hasher::new();
    for (_, bytes) in set.files {
        hasher.update(bytes);
    }
    hasher.finalize().to_hex()[..16].to_string()
}

/// The set on disk; the path of its entry script, or why it could not be written.
pub fn stage(set: &Set) -> Result<String, String> {
    let folder = crate::hosts::temp_dir().join(format!("structuregate-{}-{}", set.tag, digest(set)));
    std::fs::create_dir_all(&folder).map_err(|e| format!("IOException: {e}"))?;
    for (source, bytes) in set.files {
        let path = folder.join(name(source));
        // CONTENT-ADDRESSED, so a file of the right length is the right file.
        if std::fs::metadata(&path).is_ok_and(|m| m.len() == bytes.len() as u64) {
            continue;
        }
        std::fs::write(&path, bytes).map_err(|e| format!("IOException: {e}"))?;
    }
    Ok(folder.join(name(set.files[0].0)).to_string_lossy().into_owned())
}

/// WHAT KILLED AND EARLIER RUNS LEFT IN THE TEMP FOLDER, gone - at most once a day, because listing a busy
/// temp folder is not free: a run's own work folder or list file (named after its process id) older than six
/// hours, which no live run is, and a staged script set older than a day whose hash is not this build's.
pub fn sweep() {
    let temp = crate::hosts::temp_dir();
    let marker = temp.join("structuregate-swept");
    let age = |path: &Path| std::fs::metadata(path).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok());
    if age(&marker).is_some_and(|a| a.as_secs() < 24 * 3600) {
        return;
    }
    let _ = std::fs::write(&marker, b"");
    let current: Vec<String> = [&PSGATE, &TSGATE, &MAP_TYPESCRIPT, &MAP_PYTHON, &MAP_POWERSHELL, &TSROWS, &TSPLAIN, &PYROWS]
        .iter()
        .map(|set| format!("structuregate-{}-{}", set.tag, digest(set)))
        .collect();
    let tags: Vec<&str> = [&PSGATE, &TSGATE, &MAP_TYPESCRIPT, &MAP_PYTHON, &MAP_POWERSHELL, &TSROWS, &TSPLAIN, &PYROWS].iter().map(|s| s.tag).collect();
    let Ok(entries) = std::fs::read_dir(&temp) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        let Some(old) = age(&path) else { continue };
        if leftover(&name, old.as_secs(), &tags, &current) {
            let _ = if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
        }
    }
}

/// Whether a temp entry is this tool's and no run needs it any more.
fn leftover(name: &str, age_secs: u64, tags: &[&str], current: &[String]) -> bool {
    let Some(rest) = name.strip_prefix("structuregate-") else { return false };
    let stem = rest.strip_suffix(".txt").unwrap_or(rest);
    let last = stem.rsplit('-').next().unwrap_or("");
    // A STAGED SET FIRST: `<tag>-<16 hex>` - whose hex may be all digits, and is not a process id.
    if last.len() == 16 && last.bytes().all(|b| b.is_ascii_hexdigit()) && tags.iter().any(|tag| rest == format!("{tag}-{last}")) {
        return age_secs > 24 * 3600 && !current.iter().any(|c| c == name);
    }
    // A RUN'S OWN: `tsrows-<pid>`, `pyrows-<pid>`, `tsplain-<pid>`, `<tag>list-<pid>.txt`, `maplist-<pid>.txt`.
    let per_run = !last.is_empty() && last.bytes().all(|b| b.is_ascii_digit()) && stem.len() > last.len();
    per_run && age_secs > 6 * 3600
}

fn name(source: &str) -> &str {
    Path::new(source).file_name().and_then(|n| n.to_str()).unwrap_or(source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_what_no_run_needs_is_swept() {
        let tags = ["tsrows-node", "python rows"];
        let current = vec!["structuregate-tsrows-node-0123456789abcdef".to_string()];
        let day = 24 * 3600 + 1;
        assert!(leftover("structuregate-tsrows-4242", 7 * 3600, &tags, &current), "a killed run's folder");
        assert!(!leftover("structuregate-tsrows-4242", 3600, &tags, &current), "a run that may still be going");
        assert!(leftover("structuregate-python rowslist-77.txt", 7 * 3600, &tags, &current), "a killed run's list");
        assert!(!leftover("structuregate-tsrows-node-0123456789abcdef", day * 30, &tags, &current), "this build's set");
        assert!(leftover("structuregate-tsrows-node-1111111111111111", day, &tags, &current), "an old build's set - digits only");
        assert!(!leftover("structuregate-tsrows-node-1111111111111111", 3600, &tags, &current), "a set staged today");
        assert!(!leftover("structuregate-swept", day, &tags, &current), "the marker");
        assert!(!leftover("other-tool-4242", day, &tags, &current), "not this tool's");
    }

    #[test]
    fn a_set_is_staged_flat_under_its_hash_and_answers_with_its_entry() {
        let entry = stage(&TSPLAIN).unwrap();
        assert!(entry.ends_with("TsPlain.mjs"));
        let folder = Path::new(&entry).parent().unwrap();
        assert!(folder.join("TsResolve.mjs").is_file(), "a subfolder's script lands beside the entry");
        assert_eq!(stage(&TSPLAIN).unwrap(), entry, "the same set stages to the same folder");
    }
}
