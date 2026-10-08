//! ONE PROJECT'S INPUTS, WITHOUT MSBUILD: everything a `Compilation` needs before it can resolve a single name,
//! read from what the restore and the build already left on disk - `bin/**/<tfm>/*.dll`, `obj/project.assets.json`,
//! the referenced `.csproj`, and the framework packs. A REFERENCE THAT IS MISSING IS NOT A FAILURE: Roslyn binds
//! what it can, and `files.semantic` says per file which of the two happened.
//!
//! WHICH COPY OF AN ASSEMBLY WINS is not decided here: it needs the assembly version out of .NET metadata, so
//! the caller (Roslyn's side) chooses between `packaged` and `platform` and opens the references.

use super::disk::{self, combine};
use super::framework;
use serde_json::{json, Value};
use std::path::Path;

/// The implicit usings every SDK adds, and what each flavour adds on top - the SDK's own lists.
const BASE_USINGS: [&str; 7] = ["System", "System.Collections.Generic", "System.IO", "System.Linq", "System.Net.Http",
    "System.Threading", "System.Threading.Tasks"];
const HOST_USINGS: [&str; 4] = ["Microsoft.Extensions.Configuration", "Microsoft.Extensions.DependencyInjection",
    "Microsoft.Extensions.Hosting", "Microsoft.Extensions.Logging"];
const WEB_USINGS: [&str; 5] = ["System.Net.Http.Json", "Microsoft.AspNetCore.Builder", "Microsoft.AspNetCore.Hosting",
    "Microsoft.AspNetCore.Http", "Microsoft.AspNetCore.Routing"];

/// `consumer`: the framework of the project REFERENCING this one, which picks among its `TargetFrameworks`.
pub fn read(csproj: &str, consumer: Option<&str>) -> Option<Value> {
    let folder = disk::parent(csproj)?;
    let text = disk::text(csproj)?;
    let document = disk::parse(&text)?;
    let sdk = document.root_element().attribute("Sdk").unwrap_or("").to_string();
    let targets: Vec<String> = disk::property(&document, "TargetFramework").or_else(|| disk::property(&document, "TargetFrameworks"))
        .unwrap_or_default().split(';').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect();
    let framework = framework::pick(&targets, consumer);
    let name = disk::stem(csproj);

    let mut projects: Vec<String> = Vec::new();
    for path in referenced(&folder, &document).into_iter().chain(graph(&folder)) {
        if !projects.iter().any(|p| p.eq_ignore_ascii_case(&path)) {
            projects.push(path);
        }
    }
    // ITS OWN ASSEMBLY IS NOT A REFERENCE, and neither is a sibling's whose source is compiled here: a build
    // folder holds the project's own output, and referencing it defines every type twice - over a thousand CS0121.
    let mut exclude = vec![format!("{name}.dll")];
    exclude.extend(projects.iter().map(|p| format!("{}.dll", disk::stem(p))));

    // WHAT THE BUILD SAID IT COMPILED, when it said so since the project file last changed - see recorded.rs.
    if let Some(record) = super::recorded::find(csproj, &folder, &framework) {
        let mut built = from_record(csproj, &folder, &name, &framework, record, exclude, projects);
        // A BUILD'S Compile ITEMS NEVER HOLD A GENERATOR'S OUTPUT - it is added inside the compiler.
        if let Some(listed) = built["generated"].as_array_mut() {
            listed.extend(emitted(&folder, &framework, &document).into_iter().map(Value::from));
        }
        return Some(built);
    }
    let mut packaged = output(&folder, &framework);
    packaged.extend(restored(&folder, &framework));
    packaged.extend(framework::hinted(&folder, &document));
    // A `net4x` PROJECT COMPILES AGAINST THE .NET FRAMEWORK, not the .NET pack.
    let platform = match framework::version(&framework) {
        Some(desktop) => framework::references(&desktop, &document, &folder),
        None => framework::standard(&framework).unwrap_or_else(|| platform(&framework, &sdk, &frameworks(&document, hosted(&folder)))),
    };

    let mut generated = written(&folder, &framework);
    generated.extend(emitted(&folder, &framework, &document));
    let usings = if generated.iter().any(|g| g.to_ascii_lowercase().ends_with("globalusings.g.cs")) {
        String::new()
    } else {
        global_usings(&document, &sdk, &folder)
    };
    let attributes = if generated.iter().any(|g| g.to_ascii_lowercase().ends_with(".assemblyinfo.cs")) {
        String::new()
    } else {
        visible_to(&document, &folder, &name)
    };
    let lang_version = disk::property(&document, "LangVersion")
        .or_else(|| props(&folder).and_then(|t| disk::parse(&t).and_then(|p| disk::property(&p, "LangVersion"))))
        .unwrap_or_else(|| default_language(&framework));
    let (include, remove) = compiled(&document);
    Some(json!({
        "path": csproj, "name": name, "framework": framework, "packaged": packaged, "platform": platform, "exclude": exclude,
        "generated": generated, "usings": usings, "attributes": attributes, "lang_version": lang_version, "defines": symbols(&document),
        "files": { "explicit": sdk.is_empty(), "include": include, "remove": remove },
        // THE CLOSURE, not the direct list: `A` uses a type from `C` through `B`, and a compilation that
        // references only what the `.csproj` names answers CS0012 for every one of them.
        "projects": projects,
    }))
}

/// A project as its last build compiled it: the build's own references (the caller still picks one copy per
/// assembly name and leaves out the projects compiled here from source), its `#if` symbols and language
/// version, and its `Compile` items - those under `obj` are what the build generated (global usings, assembly
/// info, generator output), compiled and never mapped; the rest is exactly the authored file list.
fn from_record(csproj: &str, folder: &str, name: &str, framework: &str, record: super::recorded::Recorded, exclude: Vec<String>, projects: Vec<String>) -> Value {
    let obj = disk::combine(&[folder, "obj"]).to_lowercase();
    let (generated, authored): (Vec<String>, Vec<String>) = record.compile.into_iter()
        .partition(|file| file.to_lowercase().starts_with(&format!("{obj}{}", std::path::MAIN_SEPARATOR)));
    let include: Vec<String> = authored.iter()
        .map(|file| Path::new(file).strip_prefix(folder).map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_else(|_| file.clone()))
        .collect();
    let lang = if record.lang.is_empty() { default_language(framework) } else { record.lang };
    json!({
        "path": csproj, "name": name, "framework": framework, "packaged": record.references, "platform": [], "exclude": exclude,
        "generated": generated, "usings": "", "attributes": "", "lang_version": lang, "defines": record.defines,
        "files": { "explicit": true, "include": include, "remove": [] },
        "projects": projects, "recorded": true,
    })
}

/// The generated `.cs` the build wrote under `obj` for the NEWEST configuration: global usings, assembly info,
/// a generator's output. A generator that wrote nothing to disk is still invisible - the honest limit.
fn written(folder: &str, framework: &str) -> Vec<String> {
    newest_below(&combine(&[folder, "obj"]), framework, ".cs")
}

/// The assemblies the build already put next to the output - the richest source there is, read first. The
/// NEWEST configuration: a repo that last built Release must not be read through a year-old Debug folder.
fn output(folder: &str, framework: &str) -> Vec<String> {
    newest_below(&combine(&[folder, "bin"]), framework, ".dll")
}

fn newest_below(root: &str, framework: &str, extension: &str) -> Vec<String> {
    if !disk::is_dir(root) {
        return Vec::new();
    }
    disk::named_below(root, framework).and_then(disk::newest).and_then(|newest| disk::files(&newest, extension)).unwrap_or_default()
}

/// What `restore` resolved, read out of its own record - a project that has never been built still has its
/// packages in the NuGet folder.
fn restored(folder: &str, framework: &str) -> Vec<String> {
    let Some(root) = super::assets::of(folder) else { return Vec::new() };
    let folders: Vec<&String> = root.package_folders.keys().collect();
    let mut found = Vec::new();
    for target in root.targets.values() {
        for (name, library) in target {
            let Some(described) = root.libraries.get(name) else { continue };
            let kind = described.kind.as_deref().unwrap_or("");
            let relative = described.path.as_deref().unwrap_or("");
            // WHAT THE PACKAGE'S OWN build FILE REFERENCES - see targets.rs.
            if kind == "package" && let Some(build) = &library.build {
                for item in build.keys() {
                    let lower = item.to_ascii_lowercase();
                    if !lower.ends_with(".targets") && !lower.ends_with(".props") {
                        continue;
                    }
                    let item = item.replace('/', "\\");
                    if let Some(candidate) = folders.iter().map(|p| combine(&[p, relative, &item])).find(|c| disk::is_file(c)) {
                        found.extend(super::targets::references(&candidate));
                    }
                }
            }
            for item in library.compile.iter().flat_map(|c| c.keys()) {
                // `_._` is restore's "compiles against nothing"; a PROJECT reference is found through its `.csproj`.
                if item.ends_with("_._") {
                    continue;
                }
                if kind == "project" {
                    found.extend(assembly(folder, relative, framework));
                    continue;
                }
                let item = item.replace('/', "\\");
                if let Some(candidate) = folders.iter().map(|p| combine(&[p, relative, &item])).find(|c| disk::is_file(c)) {
                    found.push(candidate);
                }
            }
        }
    }
    found
}

/// The `.csproj` files this project references, as written in its own item list.
fn referenced(folder: &str, document: &roxmltree::Document) -> Vec<String> {
    disk::elements(document, "ProjectReference")
        .filter_map(|e| e.attribute("Include").filter(|i| !i.is_empty()).map(|i| disk::full(&combine(&[folder, i]))))
        .filter(|p| disk::is_file(p))
        .collect()
}

/// Every project in the restored dependency CLOSURE, transitive references included.
fn graph(folder: &str) -> Vec<String> {
    let Some(root) = super::assets::of(folder) else { return Vec::new() };
    root.libraries.values()
        .filter(|l| l.kind.as_deref() == Some("project"))
        .filter_map(|l| l.msbuild_project.as_deref().filter(|r| !r.is_empty()).map(|r| disk::full(&combine(&[folder, &r.replace('/', "\\")]))))
        .filter(|p| disk::is_file(p))
        .collect()
}

/// A project reference, through the output of the project it names.
fn assembly(folder: &str, relative: &str, framework: &str) -> Vec<String> {
    if relative.is_empty() {
        return Vec::new();
    }
    let referenced = disk::full(&combine(&[folder, &relative.replace('/', "\\")]));
    let Some(owner) = disk::parent(&referenced) else { return Vec::new() };
    let name = format!("{}.dll", disk::stem(&referenced));
    output(&owner, framework).into_iter().filter(|a| disk::file_name(a).eq_ignore_ascii_case(&name)).collect()
}

/// The shared frameworks a project asks for BY NAME - a class library using `HttpContext` declares
/// `<FrameworkReference Include="Microsoft.AspNetCore.App" />` - and the hosting one its restored closure
/// is evidence of.
fn frameworks(document: &roxmltree::Document, hosted: bool) -> Vec<String> {
    let mut named: Vec<String> = disk::elements(document, "FrameworkReference")
        .filter_map(|e| e.attribute("Include").filter(|i| !i.is_empty()).map(|i| format!("{i}.Ref")))
        .collect();
    if hosted {
        named.push("Microsoft.AspNetCore.App.Ref".into());
    }
    named
}

/// Whether this project's restored closure names anything from the hosting framework.
fn hosted(folder: &str) -> bool {
    let Some(root) = super::assets::of(folder) else { return false };
    root.libraries.keys().any(|name| {
        let lower = name.to_ascii_lowercase();
        lower.starts_with("microsoft.aspnetcore.") || lower.starts_with("microsoft.extensions.")
    })
}

/// The framework itself: in no `bin` and in no package. Without it a compilation has no `object`.
fn platform(framework: &str, sdk: &str, declared: &[String]) -> Vec<String> {
    let root = disk::dotnet(true);
    if !disk::is_dir(&root) {
        return Vec::new();
    }
    let mut packs = vec!["Microsoft.NETCore.App.Ref".to_string()];
    if sdk.to_ascii_lowercase().contains("web") {
        packs.push("Microsoft.AspNetCore.App.Ref".into());
    }
    for pack in declared {
        if !packs.iter().any(|p| p.eq_ignore_ascii_case(pack)) {
            packs.push(pack.clone());
        }
    }
    let found: Vec<String> = packs.iter()
        .filter_map(|pack| newest_pack(&combine(&[&root, "packs", pack]), Some(framework)))
        .flat_map(|reference| disk::files(&reference, ".dll").unwrap_or_default())
        .collect();
    if !found.is_empty() {
        return found;
    }
    newest_pack(&combine(&[&root, "shared", "Microsoft.NETCore.App"]), None).and_then(|s| disk::files(&s, ".dll")).unwrap_or_default()
}

/// The newest installed version folder of a pack BY VERSION ORDER (`10.0.12` above `9.0.16`), and the
/// `ref/<tfm>` inside it when one is asked for.
fn newest_pack(pack: &str, framework: Option<&str>) -> Option<String> {
    if !disk::is_dir(pack) {
        return None;
    }
    let mut versions = disk::folders(pack)?;
    // The first three `.` parts as numbers - a `-preview` part is not one, and reads as 0.
    let order = |f: &String| -> [i64; 3] {
        let parts: Vec<i64> = disk::file_name(f).split('.').map(|p| p.parse().unwrap_or(0)).collect();
        [0, 1, 2].map(|i| parts.get(i).copied().unwrap_or(0))
    };
    versions.sort_by_key(|v| std::cmp::Reverse(order(v)));
    for version in versions {
        let Some(framework) = framework else { return Some(version) };
        let reference = combine(&[&version, "ref", framework]);
        if disk::is_dir(&reference) {
            return Some(reference);
        }
    }
    None
}

/// `GlobalUsings.g.cs` as the SDK generates it: the flavour's implicit namespaces when `ImplicitUsings` is on,
/// then every `<Using Include>` with its `Alias`/`Static`, minus every `<Using Remove>`.
fn global_usings(document: &roxmltree::Document, sdk: &str, folder: &str) -> String {
    let props_text = props(folder);
    let props = props_text.as_deref().and_then(disk::parse);
    let implicitly = disk::property(document, "ImplicitUsings")
        .or_else(|| props.as_ref().and_then(|p| disk::property(p, "ImplicitUsings")))
        .is_some_and(|v| matches!(v.to_lowercase().as_str(), "enable" | "true"));
    let mut usings: Vec<String> = Vec::new();
    if implicitly {
        usings.extend(BASE_USINGS.map(String::from));
        let flavour = sdk.to_ascii_lowercase();
        if flavour.contains("web") {
            usings.extend(WEB_USINGS.iter().chain(&HOST_USINGS).map(|u| u.to_string()));
        } else if flavour.contains("worker") {
            usings.extend(HOST_USINGS.map(String::from));
        }
    }
    let mut lines: Vec<String> = Vec::new();
    let mut removed: Vec<String> = Vec::new();
    for source in props.iter().chain(std::iter::once(document)) {
        for item in disk::elements(source, "Using") {
            let include = item.attribute("Include").map(str::trim).unwrap_or("");
            if let Some(remove) = item.attribute("Remove").map(str::trim).filter(|r| !r.is_empty()) {
                removed.push(remove.to_string());
            }
            if include.is_empty() {
                continue;
            }
            let alias = item.attribute("Alias").map(str::trim).unwrap_or("");
            let is_static = item.attribute("Static").is_some_and(|s| s.trim().eq_ignore_ascii_case("true"));
            if !alias.is_empty() {
                lines.push(format!("global using {alias} = global::{include};"));
            } else if is_static {
                lines.push(format!("global using static global::{include};"));
            } else {
                usings.push(include.to_string());
            }
        }
    }
    let mut kept: Vec<String> = Vec::new();
    for using in usings {
        if !removed.contains(&using) && !kept.contains(&using) {
            kept.push(using);
        }
    }
    let mut all: Vec<String> = kept.into_iter().map(|u| format!("global using global::{u};")).collect();
    all.extend(lines);
    all.join("\n")
}

/// `AssemblyInfo.cs` as far as the SDK writes it from items: every `<InternalsVisibleTo Include>` - with its `Key` as
/// the public key - as an assembly attribute. Without it a test project calling the internals it was given got CS0122
/// on every call when no build had run. `$(AssemblyName)` and `$(MSBuildProjectName)` are this project's name.
fn visible_to(document: &roxmltree::Document, folder: &str, name: &str) -> String {
    let props_text = props(folder);
    let props = props_text.as_deref().and_then(disk::parse);
    let mut friends: Vec<String> = Vec::new();
    for source in props.iter().chain(std::iter::once(document)) {
        for item in disk::elements(source, "InternalsVisibleTo") {
            let include = item.attribute("Include").map(str::trim).unwrap_or("")
                .replace("$(AssemblyName)", name).replace("$(MSBuildProjectName)", name);
            if include.is_empty() || include.contains("$(") {
                continue;
            }
            let friend = match item.attribute("Key").map(str::trim).filter(|k| !k.is_empty()) {
                Some(key) => format!("{include}, PublicKey={key}"),
                None => include,
            };
            if !friends.contains(&friend) {
                friends.push(friend);
            }
        }
    }
    friends.iter()
        .map(|friend| format!("[assembly: global::System.Runtime.CompilerServices.InternalsVisibleTo(\"{}\")]", friend.replace('"', "")))
        .collect::<Vec<_>>().join("\n")
}

/// The C# version the SDK picks for a target framework when the project names none.
fn default_language(framework: &str) -> String {
    let tfm = framework.to_lowercase();
    if tfm.starts_with("netcoreapp3") || tfm == "netstandard2.1" {
        return "8.0".into();
    }
    // `net5.0` and later are C# 9 and one more per release; `net48`, `netstandard2.0` are 7.3.
    if tfm.starts_with("net") && !tfm.starts_with("netstandard") && !tfm.starts_with("netcoreapp") {
        let digits: String = tfm[3..].chars().take_while(char::is_ascii_digit).collect();
        let rest = &tfm[3 + digits.len()..];
        if let Ok(major) = digits.parse::<i64>()
            && rest.starts_with('.')
            && major >= 5
        {
            return format!("{}.0", major + 4);
        }
    }
    "7.3".into()
}

/// The text of the nearest `Directory.Build.props` at or above a folder, which MSBuild imports on its own.
fn props(folder: &str) -> Option<String> {
    let mut at = Some(folder.to_string());
    while let Some(here) = at {
        let candidate = combine(&[&here, "Directory.Build.props"]);
        if disk::is_file(&candidate) {
            return disk::text(&candidate).filter(|t| disk::parse(t).is_some());
        }
        at = disk::parent(&here);
    }
    None
}

/// The `#if` symbols: every `DefineConstants`, plus the two a Debug build defines. THE UNION, a choice - the
/// Debug side is the side a developer runs.
fn symbols(document: &roxmltree::Document) -> Vec<String> {
    let mut defined: std::collections::BTreeSet<String> = ["DEBUG", "TRACE"].map(String::from).into();
    for element in disk::elements(document, "DefineConstants") {
        for symbol in disk::value(element).split(';') {
            let name = symbol.trim();
            // `$(DefineConstants)` appends to whatever came before; it is a reference, not a symbol.
            if !name.is_empty() && !name.contains('$') {
                defined.insert(name.to_string());
            }
        }
    }
    defined.into_iter().collect()
}

/// What the project says it compiles: its `<Compile Include>` and `<Compile Remove>` items.
fn compiled(document: &roxmltree::Document) -> (Vec<String>, Vec<String>) {
    let (mut include, mut remove) = (Vec::new(), Vec::new());
    for element in disk::elements(document, "Compile") {
        if let Some(i) = element.attribute("Include").filter(|i| !i.is_empty()) {
            include.push(i.to_string());
        }
        if let Some(r) = element.attribute("Remove").filter(|r| !r.is_empty()) {
            remove.push(r.to_string());
        }
    }
    (include, remove)
}

/// The folder a generator would put the Razor output in - regenerated in process by `CsRazor`, so compiling the
/// build's copy too would declare every component twice.
const RAZOR_GENERATOR: &str = "Microsoft.NET.Sdk.Razor.SourceGenerators";

/// WHAT A SOURCE GENERATOR WROTE TO DISK: every `.cs` under `<newest tfm folder>/generated/**`, or under the
/// project's `<CompilerGeneratedFilesOutputPath>` - present only when the project sets
/// `<EmitCompilerGeneratedFiles>`. A generator cannot run here (NativeAOT loads no analyzer assembly), so its output
/// read off disk is the only way a Mapperly `[Mapper]` partial gets the body it binds to.
fn emitted(folder: &str, framework: &str, document: &roxmltree::Document) -> Vec<String> {
    let custom = disk::property(document, "CompilerGeneratedFilesOutputPath").filter(|p| !p.contains("$(") && !p.trim().is_empty());
    // NOTHING IS EMITTED UNLESS A PROJECT ASKS - the project or a `Directory.Build.props` above it - so `obj` is not
    // walked for a project that did not: every project's `obj` was, on every run, tens of seconds of a cold run on a large solution.
    let asked = |document: &roxmltree::Document| disk::property(document, "EmitCompilerGeneratedFiles").is_some_and(|v| v.trim().eq_ignore_ascii_case("true"));
    let props_text = props(folder);
    if custom.is_none() && !asked(document) && !props_text.as_deref().and_then(disk::parse).is_some_and(|p| asked(&p)) {
        return Vec::new();
    }
    let root = match custom {
        Some(path) if Path::new(path.trim()).is_absolute() => path.trim().to_string(),
        Some(path) => combine(&[folder, &path.trim().replace('\\', "/")]),
        None => {
            let obj = combine(&[folder, "obj"]);
            let Some(newest) = disk::named_below(&obj, framework).and_then(disk::newest) else { return Vec::new() };
            combine(&[&newest, "generated"])
        }
    };
    if !disk::is_dir(&root) {
        return Vec::new();
    }
    disk::files_below(&root, ".cs", RAZOR_GENERATOR)
}

/// A project's emitted generator output, for its fingerprint: the same files `read` compiles, for the framework the
/// project picks on its own.
pub fn emitted_of(csproj: &str) -> Vec<String> {
    let (Some(folder), Some(text)) = (disk::parent(csproj), disk::text(csproj)) else { return Vec::new() };
    let Some(document) = disk::parse(&text) else { return Vec::new() };
    let targets: Vec<String> = disk::property(&document, "TargetFramework").or_else(|| disk::property(&document, "TargetFrameworks"))
        .unwrap_or_default().split(';').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect();
    emitted(&folder, &framework::pick(&targets, None), &document)
}
