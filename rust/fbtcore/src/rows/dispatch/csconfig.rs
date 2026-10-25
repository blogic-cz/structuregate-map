//! WHAT THE C# CODE DECIDES AT RUN TIME AND SAYS NOWHERE - `structuregate.cs.json`, beside the exe or at `--cs-config` (#113).
//!
//! `typesByEnumName`: a class chosen at run time by the NAME of an enum member - a dictionary filled by reflection from
//! `Type.Name` of the types deriving from a base in a namespace, and looked up with `member.ToString()`. No parse tree
//! holds that lookup's meaning, so the consumer declares it: `[{ "enum": "Demo.Core.ReportIDs", "base": "Demo.Templates.ReportBase",
//! "namespace": "Templates.Reports", "prefixes": ["Pdf"], "ignoreCase": true }]` - `prefixes` the last namespace segments whose
//! classes are OVERRIDES of a member's default (`$"{prefix}.{name}"`), or `default` the namespace text the default classes hold
//! (`".Standard"`: every other class an override, its last namespace segment the prefix), `ignoreCase` the dictionary's comparer. Read here,
//! joined over the finished rows by `enumtypes.rs` beside it; its stamp keys the passes over them.
//!
//! `typesByName` (#121): the same dictionary reached with a STRING - a method taking the class's name as a parameter,
//! `[{ "method": "Demo.Templates.TemplateBase.Include", "parameter": "name", "base": ..., "namespace": ..., "default": ".Standard",
//! "ignoreCase": true }]` - `prefixes`/`default`/`ignoreCase` as above. Joined by `nametypes.rs`.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::path::Path;

/// One declared convention.
#[derive(Clone, Debug)]
pub struct EnumTypes {
    pub enum_symbol: String,
    pub base: String,
    pub namespace: String,
    pub prefixes: Vec<String>,
    /// `default`: the namespace text a DEFAULT class's namespace holds (`.Standard`); given, every other class is an override
    /// keyed by its last namespace segment, whatever it is - the customers need not be listed in `prefixes`.
    pub default: Option<String>,
    pub ignore_case: bool,
}

/// One declared string lookup (#121): the method, the parameter that carries the name, and how the dictionary keys its classes.
#[derive(Clone, Debug)]
pub struct NameTypes {
    pub method: String,
    pub parameter: String,
    pub base: String,
    pub namespace: String,
    pub prefixes: Vec<String>,
    pub default: Option<String>,
    pub ignore_case: bool,
}

/// The conventions, the stamp the passes are keyed by (`none` without a file), and what was wrong with the file.
pub struct CsConfig {
    pub types_by_enum_name: Vec<EnumTypes>,
    pub types_by_name: Vec<NameTypes>,
    pub stamp: String,
    pub problems: Vec<String>,
}

pub fn read(given: Option<&str>, base_dir: &str) -> CsConfig {
    let path = given.map(String::from).unwrap_or_else(|| Path::new(base_dir).join("structuregate.cs.json").to_string_lossy().into_owned());
    let mut out = CsConfig { types_by_enum_name: Vec::new(), types_by_name: Vec::new(), stamp: "none".into(), problems: Vec::new() };
    if !Path::new(&path).is_file() {
        if given.is_some() {
            out.problems.push(format!("{path} does not exist"));
        }
        return out;
    }
    let text = match std::fs::read(&path) {
        Ok(bytes) => String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(&bytes)).into_owned(),
        Err(e) => {
            out.problems.push(format!("{path} could not be read - {e}"));
            return out;
        }
    };
    out.stamp = Sha256::digest(text.as_bytes()).iter().take(8).map(|b| format!("{b:02X}")).collect();
    let root: Value = match serde_json::from_str(&crate::mapper::deep::sqlconfig::lenient(&text)) {
        Ok(root) => root,
        Err(e) => {
            out.problems.push(format!("{path} is not JSON - {e}"));
            return out;
        }
    };
    for spec in root["typesByEnumName"].as_array().into_iter().flatten() {
        let field = |k: &str| spec[k].as_str().unwrap_or("").trim().to_string();
        let (enum_symbol, base, namespace) = (field("enum"), field("base"), field("namespace"));
        if enum_symbol.is_empty() || base.is_empty() {
            out.problems.push(format!("{path}: a typesByEnumName entry needs \"enum\" and \"base\" - {spec}"));
            continue;
        }
        let prefixes = spec["prefixes"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
        let default = Some(field("default")).filter(|d| !d.is_empty());
        out.types_by_enum_name.push(EnumTypes { enum_symbol, base, namespace, prefixes, default, ignore_case: spec["ignoreCase"].as_bool().unwrap_or(false) });
    }
    for spec in root["typesByName"].as_array().into_iter().flatten() {
        let field = |k: &str| spec[k].as_str().unwrap_or("").trim().to_string();
        let (method, parameter, base, namespace) = (field("method"), field("parameter"), field("base"), field("namespace"));
        if method.is_empty() || parameter.is_empty() || base.is_empty() {
            out.problems.push(format!("{path}: a typesByName entry needs \"method\", \"parameter\" and \"base\" - {spec}"));
            continue;
        }
        let prefixes = spec["prefixes"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
        let default = Some(field("default")).filter(|d| !d.is_empty());
        out.types_by_name.push(NameTypes { method, parameter, base, namespace, prefixes, default, ignore_case: spec["ignoreCase"].as_bool().unwrap_or(false) });
    }
    out
}
