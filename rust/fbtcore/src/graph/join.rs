//! THE JOIN ITSELF: a name one file DECLARES matched against a name another file USES, plus the edges a
//! half already resolved to a path or a path suffix.

use super::File;
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// A file -> the files it reaches, both sides sorted.
pub type Rows = BTreeMap<String, Vec<String>>;

pub struct Joined {
    pub imports: Rows,
    pub soft: Rows,
    pub external: Rows,
    pub ambiguous: Rows,
    pub broken: Vec<String>,
}

/// The key a NAME is owned under. RUST NAMES LIVE APART: nothing in C#, python or a script can name a
/// rust item, nor rust theirs, so one table let a `pub struct Check` in a rust file make the C# class
/// `Check` ambiguous - the edge was dropped and `Check.cs` reported as having no reader. The other
/// languages keep sharing one table: a script naming a C# type in a string is a join drawn on purpose.
fn owned(file: &File, name: &str) -> String {
    match file.language.as_str() {
        "rust" => format!("rust::{name}"),
        // A GO NAME IS ITS PACKAGE'S, and a package is a folder: a plain name is the file's own folder's, and a use
        // through an import arrives already qualified, `dir::Name` (`gomap/`).
        "go" if name.contains("::") => format!("go:{name}"),
        "go" => format!("go:{}::{name}", folder(&file.rel)),
        _ => name.to_string(),
    }
}

pub fn resolve(files: &BTreeMap<String, File>) -> Joined {
    let mut owners: HashMap<String, Vec<String>> = HashMap::new();
    for file in files.values() {
        for name in &file.declares {
            owners.entry(owned(file, name)).or_default().push(file.rel.clone());
        }
    }
    // BY LAST SEGMENT, so a suffix join tests the handful of files that could match rather than the
    // whole tree once per unresolved import.
    let mut by_name: HashMap<&str, Vec<&str>> = HashMap::new();
    for rel in files.keys() {
        by_name.entry(rel.rsplit('/').next().unwrap_or(rel)).or_default().push(rel);
    }
    let mut joined = Joined {
        imports: Rows::new(),
        soft: Rows::new(),
        external: Rows::new(),
        ambiguous: Rows::new(),
        broken: Vec::new(),
    };
    let tables = Tables { files, owners: &owners, by_name: &by_name };
    joined.imports = tables.join(false, &mut joined);
    let mut soft = tables.join(true, &mut joined);
    // A MODULE IMPORTED BOTH WAYS IS HARD: one unguarded site is enough to need it, and listing it as
    // optional as well would say the file runs without something it plainly does not.
    for (rel, targets) in soft.iter_mut() {
        if let Some(hard) = joined.imports.get(rel) {
            targets.retain(|t| !hard.contains(t));
        }
    }
    soft.retain(|_, targets| !targets.is_empty());
    joined.soft = soft;
    joined
}

struct Tables<'a> {
    files: &'a BTreeMap<String, File>,
    owners: &'a HashMap<String, Vec<String>>,
    by_name: &'a HashMap<&'a str, Vec<&'a str>>,
}

impl Tables<'_> {
    /// One pass, over the hard uses or the optional ones - written once, because a second copy of a join
    /// this fiddly is a second place for the ambiguity rule to be got wrong. AN UNRESOLVED OPTIONAL PATH IS
    /// NOT BROKEN: naming a file that is not there is the whole point of writing an import optionally.
    fn join(&self, optional: bool, joined: &mut Joined) -> Rows {
        let mut imports = Rows::new();
        for file in self.files.values() {
            let mut edges = BTreeSet::new();
            let mut unresolved = BTreeSet::new();
            for name in if optional { &file.soft_uses } else { &file.uses } {
                let Some(targets) = self.owners.get(&owned(file, name)) else {
                    if file.reports_external {
                        unresolved.insert(name.clone());
                    }
                    continue;
                };
                // A NAME SEVERAL FILES DECLARE, BOUND BY THE COMPILER: the owners its symbols are declared in - every
                // file of a partial type - and no guess. `Result` declared three times had no edge at all, though
                // Roslyn had bound every use to one of them.
                let bound: Vec<&String> = targets.iter().filter(|t| file.bound.contains(*t)).collect();
                if targets.len() > 1 && !bound.is_empty() {
                    edges.extend(bound.into_iter().filter(|t| **t != file.rel).cloned());
                    continue;
                }
                // A GO NAME DECLARED TWICE IN ONE PACKAGE is a build-constraint variant - `_linux.go` beside `_windows.go`
                // - and each is the target in some build, so the use reaches every one. Read as ambiguous, Go's own
                // standard library reported 10 019 names no file could be joined to.
                if file.language == "go" && targets.len() > 1 {
                    edges.extend(targets.iter().filter(|t| **t != file.rel).cloned());
                    continue;
                }
                match self.pick(file, targets, Some(name)) {
                    None => {
                        joined.ambiguous.insert(name.clone(), sorted(targets.iter().cloned()));
                    }
                    Some(picked) if picked != file.rel => {
                        edges.insert(picked);
                    }
                    Some(_) => {}
                }
            }
            // A PATH IMPORT IS EXACT - the half already resolved it against the filesystem - so a path that
            // names nothing mapped is not an external package, it is a BROKEN import.
            for target in if optional { &file.soft_uses_path } else { &file.uses_path } {
                if self.files.contains_key(target) {
                    if *target != file.rel {
                        edges.insert(target.clone());
                    }
                } else if !optional {
                    joined.broken.push(format!("{}: imports {target}, which is not a file in the mapped tree", file.rel));
                }
            }
            // A SUFFIX IS RESOLVED AGAINST EVERY ROOT, which is why it is resolved here and not in the
            // half: `pkg/importer.py` is written under one root and lives under another. NOT BROKEN
            // WHEN IT MATCHES NOTHING - `numpy.linalg` is a package this tree does not hold.
            for suffix in if optional { &file.soft_uses_suffix } else { &file.uses_suffix } {
                let tail = suffix.rsplit('/').next().unwrap_or(suffix);
                let Some(sharing) = self.by_name.get(tail) else { continue };
                let ending = format!("/{suffix}");
                let hits: Vec<String> = sharing
                    .iter()
                    .filter(|rel| **rel == suffix || rel.ends_with(&ending))
                    .map(|rel| rel.to_string())
                    .collect();
                let hit = match hits.len() {
                    1 => Some(hits[0].clone()),
                    0 => None,
                    _ => self.taken(file, &hits, &module(suffix)),
                };
                match hit {
                    Some(hit) if hit != file.rel => {
                        edges.insert(hit);
                    }
                    Some(_) => {}
                    None if hits.len() > 1 => {
                        joined.ambiguous.insert(suffix.clone(), sorted(hits.into_iter()));
                    }
                    None => {}
                }
            }
            // A NAME SPELLED IN A STRING IS AN OPTIONAL EDGE, never a hard one: the evidence is that the tree
            // declares that name, weaker than an import and strong enough to stop a file being called dead.
            if optional {
                for literal in &file.literals {
                    if let Some(named) = self.owners.get(&owned(file, literal))
                        && let Some(picked) = self.pick(file, named, None)
                        && picked != file.rel
                    {
                        edges.insert(picked);
                    }
                }
            }
            if !edges.is_empty() {
                imports.insert(file.rel.clone(), edges.into_iter().collect());
            }
            if !unresolved.is_empty() {
                let listed = joined.external.entry(file.rel.clone()).or_default();
                for name in unresolved {
                    if !listed.contains(&name) {
                        listed.push(name);
                    }
                }
                listed.sort();
            }
        }
        imports
    }

    /// The ONE file a name binds to, or none when the tree cannot say. SEVERAL OWNERS STILL HAVE ONE ANSWER
    /// IN PYTHON when exactly one is a SIBLING of the importer: a bare `import util` resolves through
    /// `sys.path`, and the importer's own folder is on it. Not applied elsewhere - a C# name binds by
    /// namespace, and a folder says nothing about that.
    fn pick(&self, file: &File, targets: &[String], module: Option<&str>) -> Option<String> {
        if targets.len() == 1 {
            return Some(targets[0].clone());
        }
        if file.language != "python" {
            return None;
        }
        let home = folder(&file.rel);
        let siblings: Vec<&String> = targets.iter().filter(|t| module_folder(t) == home).collect();
        if siblings.len() == 1 {
            return Some(siblings[0].clone());
        }
        module.and_then(|m| self.taken(file, targets, m))
    }

    /// Of several files spelling a module, the ONE that binds every name the import takes from it:
    /// `from util import a, write` with three `util.py` reads the one where both exist, since the
    /// others would raise ImportError on that line. A plain `import util` has nothing to tell them by.
    fn taken(&self, file: &File, targets: &[String], module: &str) -> Option<String> {
        let taken = file.names.get(module).filter(|t| !t.is_empty())?;
        let binding: Vec<&String> = targets
            .iter()
            .filter(|t| self.files.get(*t).is_some_and(|owner| taken.is_subset(&owner.binds)))
            .collect();
        (binding.len() == 1).then(|| binding[0].clone())
    }
}

/// The same edges read the other way round - the "who would break if I change this" side.
pub fn reverse(rows: &Rows) -> Rows {
    let mut back = Rows::new();
    for (from, targets) in rows {
        for target in targets {
            back.entry(target.clone()).or_default().push(from.clone());
        }
    }
    for list in back.values_mut() {
        list.sort();
    }
    back
}

fn sorted(items: impl Iterator<Item = String>) -> Vec<String> {
    let mut all: Vec<String> = items.collect();
    all.sort();
    all
}

/// `pkg/sub.py` or `pkg/sub/__init__.py` as the dotted module it spells.
fn module(suffix: &str) -> String {
    let path = suffix
        .strip_suffix("/__init__.py")
        .or_else(|| suffix.strip_suffix(".py"))
        .unwrap_or(suffix);
    path.replace('/', ".")
}

fn folder(rel: &str) -> &str {
    rel.rfind('/').map_or("", |at| &rel[..at])
}

/// The folder a module is imported FROM: a package's `__init__.py` is its folder, one level up.
fn module_folder(rel: &str) -> &str {
    if rel.ends_with("/__init__.py") || rel == "__init__.py" { folder(folder(rel)) } else { folder(rel) }
}
