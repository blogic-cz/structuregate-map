//! `obj/project.assets.json` - what the restore resolved - READ ONCE PER FILE PER RUN, and only the fields this
//! pass asks about. The file runs to megabytes on a real solution and four readers ask it something (the packages,
//! the project closure, whether the closure is a hosted one, a net4x project's framework assemblies): parsed
//! into a whole JSON tree for each, it was four trees of several times the file's size.

use indexmap::IndexMap;
use serde::de::IgnoredAny;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Assets {
    #[serde(rename = "packageFolders")]
    pub package_folders: IndexMap<String, IgnoredAny>,
    pub libraries: IndexMap<String, Library>,
    /// `framework -> library -> what it gives this framework`, in the file's order.
    pub targets: IndexMap<String, IndexMap<String, Target>>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Library {
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub path: Option<String>,
    #[serde(rename = "msbuildProject")]
    pub msbuild_project: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Target {
    pub build: Option<IndexMap<String, IgnoredAny>>,
    pub compile: Option<IndexMap<String, IgnoredAny>>,
    #[serde(rename = "frameworkAssemblies")]
    pub framework_assemblies: Option<Vec<serde_json::Value>>,
}

/// Every file read this run, by lowercased path; None for one that is absent or not JSON.
static READ: Mutex<Option<HashMap<String, Option<Arc<Assets>>>>> = Mutex::new(None);

/// The restore record of a project folder.
pub fn of(folder: &str) -> Option<Arc<Assets>> {
    let path = super::disk::combine(&[folder, "obj", "project.assets.json"]);
    let key = path.to_lowercase();
    if let Some(known) = READ.lock().ok().and_then(|r| r.as_ref().and_then(|m| m.get(&key).cloned())) {
        return known;
    }
    let parsed = super::disk::text(&path).and_then(|text| serde_json::from_str::<Assets>(&text).ok()).map(Arc::new);
    if let Ok(mut read) = READ.lock() {
        read.get_or_insert_with(HashMap::new).insert(key, parsed.clone());
    }
    parsed
}
