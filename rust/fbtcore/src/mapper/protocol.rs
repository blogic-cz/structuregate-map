//! THE COLLECTOR every half fills, and THE ONE READER of the line protocol three embedded halves and a
//! project's own `--map-plugin` speak - they answer the same question about the same tree, and a second
//! reader would be a second place for a record to be understood differently. One record per line; a field
//! carrying `|` is folded by the producer, never escaped here. The records are listed where they are read.

use crate::graph::{self, Computed, File, Place, Shingled};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// What the halves fill in, before anything is joined.
#[derive(Default)]
pub struct Collector {
    pub files: BTreeMap<String, File>,
    /// fingerprint -> the places that share it. A body only sees whole functions, so an idiom pasted INSIDE
    /// larger ones is the expressions' to find.
    pub bodies: HashMap<String, Vec<Place>>,
    pub expressions: HashMap<String, Vec<Place>>,
    /// Each body as the SET of its statement shapes, from the halves that send one (python) - the near
    /// copies the body digest cannot see are found over these.
    pub shingled: Vec<Shingled>,
    /// Imports whose target is built at RUN TIME - not edges, and not silence either.
    pub computed: Vec<Computed>,
    pub errors: Vec<String>,
    pub notes: Vec<String>,
    pub plugin_findings: Vec<String>,
    pub halves: BTreeSet<String>,
    pub plugins: BTreeSet<String>,
    pub artifacts: BTreeMap<String, BTreeMap<String, String>>,
    /// IN THE ORDER THEY ARRIVED - the order is the dependency statement.
    pub steps: Vec<(String, BTreeMap<String, String>)>,
    pub produced: BTreeMap<String, BTreeSet<String>>,
    pub read_by: BTreeMap<String, BTreeSet<String>>,
    pub database: BTreeMap<String, i64>,
    /// -1 when no deep half ran: a run that skipped it and one that read nothing are not the same thing.
    pub reread: i64,
    pub inventory: String,
    /// WHAT THE DEEP MAP'S HALVES DID AND WHY, by half - kept in `_meta` as `last_refresh` (`deep/refresh.rs`).
    pub refresh: serde_json::Map<String, serde_json::Value>,
}

impl Collector {
    pub fn new() -> Self {
        Collector { reread: -1, ..Default::default() }
    }

    /// The record for a path a producer named, created on demand: the walk registered every file it handed
    /// over, so one named that was not given is a bug worth seeing in the map.
    pub fn row(&mut self, rel: &str, language: &str) -> &mut File {
        self.files.entry(rel.to_string()).or_insert_with(|| File {
            rel: rel.to_string(),
            language: language.to_string(),
            reports_external: true,
            ..Default::default()
        })
    }

    /// Everything collected, as the finish (`graph::finish`) takes it.
    pub fn into_input(self, options: graph::Options) -> graph::Input {
        graph::Input {
            files: self.files.into_values().collect(),
            bodies: self.bodies.into_values().collect(),
            expressions: self.expressions.into_values().collect(),
            shingled: self.shingled,
            computed: self.computed,
            errors: self.errors,
            notes: self.notes,
            plugin_findings: self.plugin_findings,
            halves: self.halves.into_iter().collect(),
            plugins: self.plugins.into_iter().collect(),
            inventory: self.inventory,
            artifacts: self.artifacts,
            steps: self.steps,
            produced: self.produced,
            read_by: self.read_by,
            database: self.database,
            reread: self.reread,
            options,
        }
    }
}

/// Records that are NOT about a file, so their second field is a name and not a path.
const NOT_ABOUT_A_FILE: [&str; 11] = [
    "MAP-FATAL", "MAP-DONE", "MAP-ARTIFACT", "MAP-STEP", "MAP-PRODUCES", "MAP-READS", "MAP-FINDING", "MAP-DB",
    "MAP-READ", "MAP-REBUILD", "MAP-NOTE",
];

/// Reads the protocol into the collector and returns which files the producer accounted for. The PREFIX is
/// how a multi-root map stays honest: a producer answers relative to the root it was GIVEN, so the folder that
/// tells two roots apart goes back on here - on the path a record is about and on any path it names.
pub fn read(output: &str, into: &mut Collector, language: &str, prefix: &str) -> HashSet<String> {
    let mut mapped = HashSet::new();
    let number = |text: &str| text.trim().parse::<i64>().ok();
    for raw in output.split('\n') {
        let line = raw.trim_end_matches('\r');
        let Some(bar) = line.find('|') else { continue };
        let record = &line[..bar];
        // Bounded, never greedy: a summary, a message and a rebuild command carry `|` of their own.
        let fields: Vec<&str> = line.splitn(6, '|').collect();
        let about_a_file = !NOT_ABOUT_A_FILE.contains(&record);
        let rel = if about_a_file && fields.len() > 1 && !fields[1].is_empty() { format!("{prefix}{}", fields[1]) } else { String::new() };
        if !rel.is_empty() {
            mapped.insert(rel.clone());
        }
        let n = fields.len();
        match record {
            // MAP-LINES|rel|n - source lines, by the producer's own counter
            "MAP-LINES" if n > 2 => {
                if let Some(lines) = number(fields[2]) {
                    into.row(&rel, language).lines = lines;
                }
            }
            // MAP-SUMMARY|rel|text - the file's headline (a docstring, a doc comment)
            "MAP-SUMMARY" if n > 2 => into.row(&rel, language).summary = fields[2].into(),
            // MAP-DECL / MAP-USE - a name another file can reach this one by; a name this file uses
            "MAP-DECL" if n > 2 => {
                into.row(&rel, language).declares.insert(fields[2].into());
            }
            "MAP-USE" if n > 2 => {
                into.row(&rel, language).uses.insert(fields[2].into());
            }
            // MAP-PATH / MAP-SOFTPATH - an import the producer ALREADY resolved to a file (the soft one OPTIONAL)
            "MAP-PATH" if n > 2 => {
                into.row(&rel, language).uses_path.insert(format!("{prefix}{}", fields[2]));
            }
            "MAP-SOFT" if n > 2 => {
                into.row(&rel, language).soft_uses.insert(fields[2].into());
            }
            "MAP-SOFTPATH" if n > 2 => {
                into.row(&rel, language).soft_uses_path.insert(format!("{prefix}{}", fields[2]));
            }
            // NO PREFIX ON A SUFFIX: the whole point of one is that the file it names is under a DIFFERENT
            // root, and stamping this one's prefix on it would guarantee the match fails.
            "MAP-SUFFIX" if n > 2 => {
                into.row(&rel, language).uses_suffix.insert(fields[2].into());
            }
            "MAP-SOFTSUFFIX" if n > 2 => {
                into.row(&rel, language).soft_uses_suffix.insert(fields[2].into());
            }
            // MAP-LITERAL - a string that spells a name: a string-keyed lookup leaves no import behind
            "MAP-LITERAL" if n > 2 => {
                into.row(&rel, language).literals.insert(fields[2].into());
            }
            // MAP-NAMES|rel|module|a b c - what an unresolved `from module import a, b, c` takes
            "MAP-NAMES" if n > 3 => {
                let names = into.row(&rel, language).names.entry(fields[2].into()).or_default();
                names.extend(fields[3].split(' ').filter(|s| !s.is_empty()).map(String::from));
            }
            // MAP-BINDS|rel|a b c - every name this file binds at module level
            "MAP-BINDS" if n > 2 => {
                let binds = &mut into.row(&rel, language).binds;
                binds.extend(fields[2].split(' ').filter(|s| !s.is_empty()).map(String::from));
            }
            // MAP-REGISTERED|rel|line|decorator - a def handed to an object, so the file is entered through it
            "MAP-REGISTERED" if n > 3 => {
                into.row(&rel, language).registered.insert(fields[3].into());
            }
            "MAP-ENTRY" => into.row(&rel, language).entry = true,
            "MAP-UNMAPPED" if n > 2 => into.row(&rel, language).unmapped = fields[2].into(),
            // MAP-BODY|rel|line|name|digest|size[|shingles] and MAP-EXPR|rel|line|digest|size - fingerprints.
            // The seventh field is OPTIONAL: one 8-hex digest per statement, space-joined, from the python
            // half alone today; the other halves send six fields and are the duplicates' only. THE SPLIT
            // ABOVE STOPS AT SIX, so the sixth slot is `size` or `size|shingles` and is split once more here -
            // read as `number(fields[5])` it was None, and every python body silently left the map.
            "MAP-BODY" if n > 5 => {
                let (size, shingles) = fields[5].split_once('|').unwrap_or((fields[5], ""));
                if let Some(size) = number(size) {
                    let place = format!("{rel}:{}:{}", fields[2], fields[3]);
                    into.bodies.entry(fields[4].into()).or_default().push(Place(place.clone(), size as usize));
                    if !shingles.trim().is_empty() {
                        let shingles: BTreeSet<String> = shingles.split(' ').filter(|s| !s.is_empty()).map(String::from).collect();
                        into.shingled.push(Shingled { place, size: size as usize, digest: fields[4].into(), shingles: shingles.into_iter().collect() });
                    }
                }
            }
            "MAP-EXPR" if n > 4 => {
                if let Some(size) = number(fields[4]) {
                    into.expressions.entry(fields[3].into()).or_default().push(Place(format!("{rel}:{}", fields[2]), size as usize));
                }
            }
            "MAP-COMPUTED" if n > 3 => into.computed.push(Computed { rel: rel.clone(), line: fields[2].into(), what: fields[3].into() }),
            // MAP-DB|table|rows - one table written into the SQLite map (-1: no FTS5 here)
            "MAP-DB" if n > 2 => {
                if let Some(rows) = number(fields[2]) {
                    into.database.insert(fields[1].into(), rows);
                }
            }
            // MAP-READ|parsed|total - SUMMED: a mixed tree has two deep halves writing one database.
            "MAP-READ" if n > 2 => {
                if let Some(reread) = number(fields[1]) {
                    into.reread = into.reread.max(0) + reread;
                }
            }
            // MAP-REBUILD - not an error, but the slow path, and one every turn is a bug in the incremental rule
            "MAP-REBUILD" if n > 2 => into.notes.push(format!("the deep map was rebuilt from scratch: {} {}", fields[1], fields[2])),
            "MAP-NOTE" if n > 1 => {
                if !into.notes.iter().any(|note| note == fields[1]) {
                    into.notes.push(fields[1].into());
                }
            }
            "MAP-ERROR" if n > 3 => into.errors.push(format!("UNPARSED  {rel}:{}: {}", fields[2], fields[3])),
            // WHAT ONLY THE PROJECT CAN STATE. A field name is not an enum: whatever it declares lands unchanged.
            "MAP-ARTIFACT" if n > 3 => {
                into.artifacts.entry(fields[1].into()).or_default().insert(fields[2].into(), fields[3].into());
            }
            "MAP-STEP" if n > 3 => {
                let at = match into.steps.iter().position(|(name, _)| name == fields[1]) {
                    Some(at) => at,
                    None => {
                        into.steps.push((fields[1].into(), BTreeMap::new()));
                        into.steps.len() - 1
                    }
                };
                into.steps[at].1.insert(fields[2].into(), fields[3].into());
            }
            "MAP-PRODUCES" if n > 2 => {
                into.produced.entry(fields[1].into()).or_default().insert(fields[2].into());
            }
            "MAP-READS" if n > 2 => {
                into.read_by.entry(fields[1].into()).or_default().insert(fields[2].into());
            }
            // The SEVERITY is the producer's to state. The prefixes differ in case on purpose: the error test
            // is ordinal, so a note can never be mistaken for one.
            "MAP-FINDING" if n > 2 => into.plugin_findings.push(if fields[1] == "error" {
                format!("PLUGIN    {}", fields[2])
            } else {
                format!("plugin    {}", fields[2])
            }),
            _ => {}
        }
    }
    mapped
}

/// The first line of a producer's output carrying a prefix, without it.
pub fn first(output: &str, prefix: &str) -> Option<String> {
    output.split('\n').map(|l| l.trim_end_matches('\r')).find_map(|l| l.strip_prefix(prefix).map(str::to_string))
}
