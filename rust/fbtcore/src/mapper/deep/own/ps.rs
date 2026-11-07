//! THE POWERSHELL HALF OF THE DEEP MAP: `PsRows.ps1` under the `--ps-host`, a payload half shaped like python's
//! (`payload.rs`) - every `.ps1 .psm1 .psd1` file's functions, calls, literals, expressions and regexes, stamped
//! `files.lang = 'powershell'`. It shares `PsShape.ps1` with the file map, so its `body_shape` and `shape` are the
//! digests buildmap.json groups by.
//!
//! A TREE WHOSE LAST POWERSHELL FILE HAS GONE launches nothing: the store is told those rows are gone (`payload::forget`).

use crate::mapper::protocol::Collector;
use super::super::{handed, hashes, key, payload, Deep, Files};
use std::path::Path;

pub(crate) fn run(into: &mut Collector, deep: &Deep, hashes: &hashes::Hashes, files: &Files) {
    let db = deep.db.as_str();
    if files.powershell.is_empty() {
        if Path::new(db).is_file() {
            payload::forget(into, "powershell rows", "powershell", db, &deep.root, "the powershell rows were dropped: no PowerShell file is mapped now");
        }
        return;
    }
    let before: Vec<String> = crate::mapper::POWERSHELL_BEFORE.map(String::from).to_vec();
    let extra: Vec<String> = vec!["--hashes".into(), "hashes.json".into()];
    let arguments = [before.clone(), extra.clone()].concat();
    let half = payload::Half {
        language: "powershell rows",
        lang: "powershell",
        folder: "psrows",
        script: deep.staged("powershell rows"),
        host: &deep.ps_host,
        flag: "--ps-host",
        before,
        extra,
        beside: vec![("hashes.json", handed(hashes, &files.powershell))],
        named: "the powershell half",
        key: key(hashes, &crate::embedded::PSROWS, &deep.ps_host, &arguments, &files.powershell, "", None),
    };
    payload::run(into, &half, &files.powershell, db, &deep.root, None);
}
