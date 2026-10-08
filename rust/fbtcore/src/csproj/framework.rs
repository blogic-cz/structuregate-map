//! WHAT A PROJECT COMPILES AGAINST THAT IS NEITHER A PACKAGE NOR A .NET PACK: the .NET FRAMEWORK reference
//! assemblies of a `net4x` project, the reference assemblies of a `netstandard2.x` one, and every
//! `<Reference HintPath>` a project names by hand.
//!
//! WHY. A `net4x` project compiled against the .NET (Core) pack, where `System.Web` and `HttpContext` do not
//! exist, fails on its references rather than its code. And a `HintPath` has to be
//! read, or the vendor assemblies a project keeps in its own `lib\` are "not found". READ FROM DISK, AS
//! THE BUILD WOULD - the targeting pack, or the `microsoft.netframework.referenceassemblies.*` package.

use super::disk::{self, combine};
use serde_json::Value;

/// What the SDK references in a .NET Framework project without being asked.
const IMPLICIT: [&str; 10] = ["mscorlib", "System", "System.Core", "System.Data", "System.Drawing", "System.IO.Compression.FileSystem",
    "System.Numerics", "System.Runtime.Serialization", "System.Xml", "System.Xml.Linq"];

/// Which of a multi-targeted project's frameworks a CONSUMER compiles against - NuGet's nearest match: the
/// consumer's own, else the newest of its kind not newer than it, else the newest `netstandard`, else the
/// first. A multi-targeted library (`net10.0;netstandard2.0`) was always compiled as its first, and a `net472`
/// consumer met System.Runtime 10.0 and CS7069.
pub fn pick(frameworks: &[String], consumer: Option<&str>) -> String {
    let Some(first) = frameworks.first() else { return "net10.0".into() };
    let Some(consumer) = consumer.filter(|_| frameworks.len() > 1) else { return first.clone() };
    if let Some(exact) = frameworks.iter().find(|f| f.eq_ignore_ascii_case(consumer)) {
        return exact.clone();
    }
    let (kind, wanted) = (kind(consumer), number(consumer));
    if kind != "other"
        && let Some(same) = newest(frameworks.iter().filter(|f| self::kind(f) == kind && number(f) <= wanted))
    {
        return same;
    }
    newest(frameworks.iter().filter(|f| self::kind(f) == "standard")).unwrap_or_else(|| first.clone())
}

/// The highest-numbered of some frameworks; the first listed wins a tie.
fn newest<'a>(frameworks: impl Iterator<Item = &'a String>) -> Option<String> {
    let listed: Vec<&String> = frameworks.collect();
    let best = listed.iter().map(|f| number(f)).max()?;
    listed.into_iter().find(|f| number(f) == best).cloned()
}

fn kind(framework: &str) -> &'static str {
    let lower = framework.to_ascii_lowercase();
    if version(framework).is_some() {
        "desktop"
    } else if lower.starts_with("netstandard") {
        "standard"
    } else if lower.starts_with("netcoreapp") || (lower.starts_with("net") && framework.contains('.')) {
        "core"
    } else {
        "other"
    }
}

/// A framework's version as one comparable number: `net472` 472, `net10.0` 1000, `netstandard2.1` 210.
fn number(framework: &str) -> i64 {
    let digits: String = framework.chars().skip_while(|c| !c.is_ascii_digit()).take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    if version(framework).is_some() {
        return format!("{digits:0<3}").parse().unwrap_or(0);
    }
    let parts: Vec<&str> = digits.split('.').collect();
    let at = |i: usize| parts.get(i).and_then(|p| p.parse::<i64>().ok()).unwrap_or(0);
    at(0) * 100 + at(1) * 10
}

/// `v4.7.2` for `net472`, `v4.8` for `net48`; None for anything that is not .NET Framework.
pub fn version(framework: &str) -> Option<String> {
    if !framework.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("net")) {
        return None;
    }
    let digits = &framework[3..];
    if !(2..=3).contains(&digits.len()) || !digits.bytes().all(|b| b.is_ascii_digit()) || !digits.starts_with('4') {
        return None;
    }
    Some(format!("v{}", digits.chars().map(String::from).collect::<Vec<_>>().join(".")))
}

/// The framework assemblies a `net4x` project compiles against: the implicit set, every `<Reference Include>`
/// without a HintPath, every `frameworkAssemblies` a restored package asks for, the facades, and the SDK's
/// .NET Standard support facades - or nothing when no targeting pack for that version is on this machine.
pub fn references(version: &str, document: &roxmltree::Document, project: &str) -> Vec<String> {
    let Some(folder) = pack(version) else { return Vec::new() };
    let mut names: Vec<String> = Vec::new();
    let mut add = |name: String| {
        if !names.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
            names.push(name);
        }
    };
    IMPLICIT.iter().for_each(|n| add((*n).to_string()));
    items(document).into_iter().filter(|(_, hint)| hint.is_none()).for_each(|(include, _)| add(include));
    asked(project).into_iter().for_each(&mut add);
    let mut found: Vec<String> = names.iter().map(|n| combine(&[&folder, &format!("{n}.dll")])).filter(|d| disk::is_file(d)).collect();
    found.extend(dlls(&combine(&[&folder, "Facades"])));
    // THE SDK'S .NET STANDARD SUPPORT FACADES: their `System.Runtime` forwards what a netstandard type names.
    for sdk in versions(&combine(&[&disk::dotnet(false), "sdk"])) {
        let extensions = combine(&[&sdk, "Microsoft", "Microsoft.NET.Build.Extensions", "net461", "lib"]);
        if disk::is_dir(&extensions) {
            found.extend(dlls(&extensions));
            break;
        }
    }
    found
}

fn dlls(folder: &str) -> Vec<String> {
    if !disk::is_dir(folder) {
        return Vec::new();
    }
    disk::files(folder, ".dll").unwrap_or_default()
}

/// The reference assemblies of a `netstandard2.x` project: `netstandard.dll` and its facades - NOT the runtime,
/// whose `System.Private.CoreLib` then leaked into every `net472` consumer as CS0012, hundreds of times in one project.
pub fn standard(framework: &str) -> Option<Vec<String>> {
    if !framework.get(..11).is_some_and(|p| p.eq_ignore_ascii_case("netstandard")) {
        return None;
    }
    let mut candidates: Vec<String> = versions(&combine(&[&disk::dotnet(false), "packs", "NETStandard.Library.Ref"]))
        .into_iter().map(|v| combine(&[&v, "ref", framework])).collect();
    candidates.extend(versions(&combine(&[&disk::packages(), "netstandard.library"])).into_iter().map(|v| combine(&[&v, "build", framework, "ref"])));
    candidates.into_iter().filter(|f| disk::is_file(&combine(&[f, "netstandard.dll"]))).find_map(|f| disk::files(&f, ".dll"))
}

/// The version folders of a package or pack, newest first by their numbers; the listing order breaks a tie.
pub fn versions(folder: &str) -> Vec<String> {
    if !disk::is_dir(folder) {
        return Vec::new();
    }
    let mut listed = disk::folders(folder).unwrap_or_default();
    listed.sort_by(|a, b| disk::version_parts(b).cmp(&disk::version_parts(a)));
    listed
}

/// The `frameworkAssemblies` of every package in the project's `project.assets.json`.
fn asked(project: &str) -> Vec<String> {
    let Some(assets) = super::assets::of(project) else { return Vec::new() };
    assets.targets.values()
        .flat_map(|target| target.values())
        .flat_map(|library| library.framework_assemblies.iter().flatten())
        .filter_map(Value::as_str).filter(|s| !s.is_empty()).map(String::from).collect()
}

/// Every `<Reference>` with a `HintPath` that exists, relative to the project. A path holding an MSBuild
/// property this pass cannot expand is skipped rather than guessed.
pub fn hinted(folder: &str, document: &roxmltree::Document) -> Vec<String> {
    items(document).into_iter()
        .filter_map(|(_, hint)| hint)
        .filter(|hint| !hint.contains("$("))
        .map(|hint| disk::full(&if disk::rooted(&hint) { hint.clone() } else { combine(&[folder, &hint]) }))
        .filter(|path| disk::is_file(path))
        .collect()
}

/// `(assembly name, HintPath)` of every `<Reference>` item; only the name before a strong name's comma.
fn items(document: &roxmltree::Document) -> Vec<(String, Option<String>)> {
    disk::elements(document, "Reference")
        .filter_map(|element| {
            let include = element.attribute("Include")?.split(',').next()?.trim().to_string();
            if include.is_empty() {
                return None;
            }
            let hint = disk::child(element, "HintPath").map(|h| disk::value(h).trim().to_string()).filter(|h| !h.is_empty());
            Some((include, hint))
        })
        .collect()
}

/// The targeting pack for `version`: the installed one, else the newest reference-assemblies package.
fn pack(version: &str) -> Option<String> {
    let x86 = std::env::var("ProgramFiles(x86)").unwrap_or_default();
    let installed = combine(&[&x86, "Reference Assemblies", "Microsoft", "Framework", ".NETFramework", version]);
    if disk::is_file(&combine(&[&installed, "mscorlib.dll"])) {
        return Some(installed);
    }
    let package = combine(&[&disk::packages(), &format!("microsoft.netframework.referenceassemblies.net{}", version[1..].replace('.', ""))]);
    versions(&package).into_iter()
        .map(|candidate| combine(&[&candidate, "build", ".NETFramework", version]))
        .find(|inside| disk::is_file(&combine(&[inside, "mscorlib.dll"])))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_consumer_gets_the_nearest_framework_of_its_kind() {
        let targets = ["net10.0".to_string(), "netstandard2.0".to_string()];
        assert_eq!(pick(&targets, Some("net472")), "netstandard2.0");
        assert_eq!(pick(&targets, Some("net9.0")), "netstandard2.0");
        assert_eq!(pick(&targets, Some("net10.0")), "net10.0");
        assert_eq!(pick(&targets, None), "net10.0");
        assert_eq!(version("net472").as_deref(), Some("v4.7.2"));
        assert_eq!(version("net10.0"), None);
    }
}
