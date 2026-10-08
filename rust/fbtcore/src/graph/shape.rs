//! WHAT THE EDGES ADD UP TO: import cycles, the bodies and expressions written more than once, and the
//! files nothing reads.

use super::join::{Joined, Rows};
use super::{File, Place, Shingled};

use std::collections::{BTreeMap, BTreeSet, HashMap};

/// One repeated shape and every place it is written. SIZE IS IN SOURCE CHARACTERS in every language, so
/// groups from different halves rank in one list.
pub struct Group {
    pub size: usize,
    pub at: Vec<String>,
}

/// How many cycles are worth printing before the list stops being read.
const MAX_CYCLES: usize = 20;

/// The import cycles, by depth-first search. REPORTED, NEVER FAILED: in C# a file-level cycle is ordinary
/// (the compiler sees one compilation), in python and TypeScript it is a load-order hazard - naming it is
/// the honest answer either way. One cycle per SET of files: the same loop entered from each member is one
/// thing to fix.
pub fn cycles(imports: &Rows) -> Vec<Vec<String>> {
    struct Search<'a> {
        imports: &'a Rows,
        state: HashMap<&'a str, u8>,
        path: Vec<&'a str>,
        seen: BTreeSet<String>,
        found: Vec<Vec<String>>,
    }
    impl<'a> Search<'a> {
        fn walk(&mut self, node: &'a str) {
            if self.found.len() >= MAX_CYCLES {
                return;
            }
            self.state.insert(node, 1);
            self.path.push(node);
            let imports = self.imports;
            for next in imports.get(node).map(Vec::as_slice).unwrap_or_default() {
                match self.state.get(next.as_str()).copied().unwrap_or(0) {
                    1 => {
                        let start = self.path.iter().position(|p| *p == next).unwrap_or(0);
                        let cycle: Vec<String> = self.path[start..].iter().map(|p| p.to_string()).collect();
                        let mut key = cycle.clone();
                        key.sort();
                        if self.seen.insert(key.concat()) {
                            self.found.push(cycle);
                        }
                    }
                    0 => self.walk(next),
                    _ => {}
                }
            }
            self.path.pop();
            self.state.insert(node, 2);
        }
    }
    let mut search = Search { imports, state: HashMap::new(), path: Vec::new(), seen: BTreeSet::new(), found: Vec::new() };
    for node in imports.keys() {
        if search.state.get(node.as_str()).copied().unwrap_or(0) == 0 {
            search.walk(node);
        }
    }
    search.found
}

/// The fingerprints that repeat, as GROUPS: one body in five files is one thing to fix, and ten pairs
/// would rank it below five unrelated pairs.
pub fn bodies(found: &[Vec<Place>]) -> Vec<Group> {
    let mut groups: Vec<(usize, Vec<String>)> = found
        .iter()
        .filter(|at| at.len() >= 2)
        .map(|at| (at.iter().map(|p| p.1).max().unwrap_or(0), sorted_places(at)))
        .collect();
    order(&mut groups);
    groups.into_iter().map(|(size, at)| Group { size, at }).collect()
}

/// Two bodies that share MOST of their statement shapes but not all - one literal changed, one line added -
/// which the whole-body digest reads as two unrelated functions. Scored, never grouped.
pub struct Similar {
    /// Shared shapes as a percentage of the union of both sets.
    pub score: usize,
    pub shared: usize,
    pub total: usize,
    pub size: usize,
    pub at: Vec<String>,
}

/// Fewer shared shapes than this is coincidence: `x = f(y)` and `return x` are in every function.
const MIN_SHARED: usize = 3;
/// One statement added to a four-statement body is 80; to a five-statement body 83. Below this, two
/// functions share a prefix, not a purpose.
const MIN_SCORE: usize = 75;
/// A shape in more functions than this is vocabulary, and pairing every two of them is quadratic noise.
const MAX_FREQUENCY: usize = 50;

/// The near copies, over an inverted index (shape -> the bodies holding it) so only bodies that share a
/// shape are ever compared. A pair whose body digests are EQUAL is a duplicate, already reported, and is
/// left out here.
pub fn similar(found: &[Shingled]) -> Vec<Similar> {
    let mut index: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, body) in found.iter().enumerate() {
        for shingle in &body.shingles {
            index.entry(shingle.as_str()).or_default().push(i);
        }
    }
    let mut shared: HashMap<(usize, usize), usize> = HashMap::new();
    for owners in index.values().filter(|o| o.len() >= 2 && o.len() <= MAX_FREQUENCY) {
        for (k, &a) in owners.iter().enumerate() {
            for &b in &owners[k + 1..] {
                *shared.entry((a.min(b), a.max(b))).or_insert(0) += 1;
            }
        }
    }
    let mut pairs: Vec<Similar> = shared
        .into_iter()
        .filter_map(|((a, b), n)| {
            let (one, two) = (&found[a], &found[b]);
            if n < MIN_SHARED || one.digest == two.digest {
                return None;
            }
            let total = one.shingles.len() + two.shingles.len() - n;
            let score = n * 100 / total;
            if score < MIN_SCORE {
                return None;
            }
            let mut at = vec![one.place.clone(), two.place.clone()];
            at.sort();
            Some(Similar { score, shared: n, total, size: one.size.max(two.size), at })
        })
        .collect();
    pairs.sort_by(|x, y| {
        y.score.cmp(&x.score).then(y.shared.cmp(&x.shared)).then(y.size.cmp(&x.size)).then_with(|| x.at.cmp(&y.at))
    });
    pairs
}

/// The same for expressions, narrowed twice: a shape repeated inside ONE file is a local choice, so a
/// group must span two files; and a subexpression repeats wherever its parent does, so of the shapes
/// sharing one set of places only the WIDEST is kept.
pub fn expressions(found: &[Vec<Place>]) -> Vec<Group> {
    let mut widest: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    for at in found.iter().filter(|at| at.len() >= 2) {
        let places = sorted_places(at);
        let files: BTreeSet<&str> = places.iter().map(|p| file_of(p)).collect();
        if files.len() < 2 {
            continue;
        }
        let size = at.iter().map(|p| p.1).max().unwrap_or(0);
        let key = places.concat();
        if widest.get(&key).is_none_or(|held| size > held.0) {
            widest.insert(key, (size, places));
        }
    }
    let mut groups: Vec<(usize, Vec<String>)> = widest.into_values().collect();
    order(&mut groups);
    groups.into_iter().map(|(size, at)| Group { size, at }).collect()
}

/// Most places first, then the widest, then by where - and the whole list last, so a tie has one order.
fn order(groups: &mut [(usize, Vec<String>)]) {
    groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(b.0.cmp(&a.0)).then_with(|| a.1.cmp(&b.1)));
}

fn sorted_places(at: &[Place]) -> Vec<String> {
    let mut places: Vec<String> = at.iter().map(|p| p.0.clone()).collect();
    places.sort();
    places
}

/// The file part of a `path:line` or `path:line:name` place, split from the LEFT: a path is already `/`
/// here, and the line is always the second field.
fn file_of(place: &str) -> &str {
    place.split(':').next().unwrap_or(place)
}

/// Every file the tree declares something in and nothing in the tree names - dead, or reached in a way no
/// parse tree can see. NOT one of them: an entry point (loaded, not imported), a file that hands its defs
/// to an object (`@app.route` - entered through that object), a file read only OPTIONALLY (a degradation
/// path loads it on purpose), and a file an AMBIGUOUS use may bind to (something plainly names it).
pub fn unread(files: &BTreeMap<String, File>, joined: &Joined) -> Vec<String> {
    let candidates: BTreeSet<&String> = joined.ambiguous.values().flatten().collect();
    let read: BTreeSet<&String> = joined.imports.values().chain(joined.soft.values()).flatten().collect();
    files
        .values()
        .filter(|f| f.unmapped.is_empty() && !f.entry && !f.declares.is_empty() && f.registered.is_empty())
        .filter(|f| !read.contains(&f.rel) && !candidates.contains(&f.rel))
        .map(|f| f.rel.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(pairs: &[(&str, &[&str])]) -> Rows {
        pairs.iter().map(|(k, v)| (k.to_string(), v.iter().map(|s| s.to_string()).collect())).collect()
    }

    #[test]
    fn a_loop_is_one_cycle_however_many_of_its_members_it_is_entered_from() {
        let found = cycles(&rows(&[("a", &["b"]), ("b", &["c"]), ("c", &["a"]), ("d", &["a"])]));
        assert_eq!(found, vec![vec!["a".to_string(), "b".to_string(), "c".to_string()]]);
    }

    #[test]
    fn a_near_copy_is_scored_a_loose_one_is_not_and_an_identical_one_is_the_duplicates() {
        let body = |place: &str, digest: &str, shingles: &[&str]| Shingled {
            place: place.to_string(),
            size: 10,
            digest: digest.to_string(),
            shingles: shingles.iter().map(|s| s.to_string()).collect(),
        };
        let found = vec![
            body("a.py:1:f", "d1", &["s1", "s2", "s3", "s4"]),
            body("b.py:1:g", "d2", &["s1", "s2", "s3", "s4", "s5"]),
            body("c.py:1:h", "d3", &["s1", "s4", "s8", "s9"]),
            body("d.py:1:k", "d1", &["s1", "s2", "s3", "s4"]),
        ];
        let pairs = similar(&found);
        // a-b shares 4 of 5 (80); a-d is the same digest (the duplicates' row); c shares 2 with a and b.
        let at: Vec<Vec<String>> = pairs.iter().map(|p| p.at.clone()).collect();
        assert_eq!(at, vec![
            vec!["a.py:1:f".to_string(), "b.py:1:g".to_string()],
            vec!["b.py:1:g".to_string(), "d.py:1:k".to_string()],
        ]);
        assert_eq!((pairs[0].score, pairs[0].shared, pairs[0].total), (80, 4, 5));
    }

    #[test]
    fn an_expression_inside_one_file_is_not_a_duplicate_and_the_widest_shape_wins() {
        let place = |w: &str, s| Place(w.to_string(), s);
        let found = vec![
            vec![place("a.py:1", 30), place("a.py:9", 30)],
            vec![place("a.py:1", 40), place("b.py:2", 40)],
            vec![place("a.py:1", 12), place("b.py:2", 12)],
        ];
        let groups = expressions(&found);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].size, 40);
    }
}
