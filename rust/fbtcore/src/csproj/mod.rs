//! WHAT A C# PROJECT COMPILES AGAINST, worked out WITHOUT MSBUILD - this runs inside an MSBuild target, and
//! an MSBuild inside one is both slow and circular. The project file, `Directory.Build.props`, the restore's
//! `project.assets.json`, the build's `bin`/`obj`, the framework packs and a package's own `build/*.targets`
//! are READ; Roslyn, on the caller's side, is handed the answer and builds the compilation.
//!
//! One door, `fbt_cs_ask`: `{owner: file}` (the nearest `.csproj`), `{project: csproj, consumer}` (its
//! inputs), and `{inputs: file}` (what a `.razor`/`.cshtml` is compiled with besides its own text).

mod assets;
mod disk;
mod framework;
mod project;
mod recorded;
mod targets;

use crate::{in_string, out_string};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::ffi::c_char;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::Mutex;

/// The nearest `.csproj` per FOLDER: a project of 400 files would walk one directory chain 400 times.
static OWNERS: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);

/// # Safety
/// `question` must be null or NUL-terminated UTF-8. The answer is the caller's to free with `fbt_string_free`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fbt_cs_ask(question: *const c_char) -> *mut c_char {
    let text = unsafe { in_string(question) }.unwrap_or_default();
    let answer = catch_unwind(AssertUnwindSafe(|| {
        let question: Value = serde_json::from_str(&text).unwrap_or_default();
        if let Some(file) = question["owner"].as_str() {
            json!({ "owner": owner(file) })
        } else if let Some(csproj) = question["project"].as_str() {
            project::read(csproj, question["consumer"].as_str()).unwrap_or(Value::Null)
        } else if let Some(file) = question["inputs"].as_str() {
            json!({ "inputs": inputs(file) })
        } else {
            Value::Null
        }
    }));
    out_string(answer.unwrap_or(Value::Null).to_string())
}

/// A project the SDK builds - `<Project Sdk=...>` or an `<Sdk>` element - which is restored into `obj/project.assets.json`.
/// An old-style project restores `packages.config` elsewhere, and is never called unrestored.
pub fn sdk_style(csproj: &str) -> bool {
    let Some(text) = disk::text(csproj) else { return false };
    let Some(document) = disk::parse(&text) else { return false };
    document.root_element().attribute("Sdk").is_some() || disk::elements(&document, "Sdk").next().is_some()
}

pub use project::emitted_of;

/// The folder a project sits in.
pub fn parent(csproj: &str) -> Option<String> {
    disk::parent(csproj)
}

/// The nearest `.csproj` at or above a file - which decides its compilation, as the build decides it.
pub fn owner(file: &str) -> Option<String> {
    let folder = disk::parent(file)?;
    let key = folder.to_lowercase();
    if let Some(known) = OWNERS.lock().ok().and_then(|o| o.as_ref().and_then(|m| m.get(&key).cloned())) {
        return known;
    }
    let mut found = None;
    let mut at = Some(folder);
    while let Some(here) = at {
        let Some(projects) = disk::files(&here, ".csproj") else { break };
        if let Some(first) = projects.into_iter().next() {
            found = Some(first);
            break;
        }
        at = disk::parent(&here);
    }
    if let Ok(mut owners) = OWNERS.lock() {
        owners.get_or_insert_with(HashMap::new).insert(key, found.clone());
    }
    found
}

/// What a markup file is compiled WITH besides its own text - every `Web.config`, `_Imports.razor` and
/// `_ViewImports.cshtml` from its folder up to its project - folded into its sha, so an edit there re-reads
/// the files below it.
pub fn inputs(file: &str) -> Vec<String> {
    let top = owner(file).and_then(|p| disk::parent(&p));
    let mut found = Vec::new();
    let mut at = disk::parent(file);
    while let Some(here) = at {
        for name in ["Web.config", "_Imports.razor", "_ViewImports.cshtml"] {
            let candidate = disk::combine(&[&here, name]);
            if disk::is_file(&candidate) {
                found.push(candidate);
            }
        }
        match &top {
            Some(top) if !disk::full(&here).eq_ignore_ascii_case(&disk::full(top)) => {}
            _ => break,
        }
        at = disk::parent(&here);
    }
    found
}
