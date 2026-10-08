//! WHICH DOCS A SESSION CAN FIND. A session starts from what the harness loads without being asked - a CLAUDE.md
//! or AGENTS.md, and everything under a `.claude/` folder (agents, skills, rules, commands) - and learns of any
//! other doc only by following what those name. A doc no chain of mentions reaches from there is `DOC-ORPHAN`:
//! written, kept, and read by nobody who was not already looking for it.
//!
//! A MENTION IS WHAT THE MARKDOWN HALF RESOLVED (`mdmap/`): a link, a path in code, a path under a `cd` - each
//! followed the way a reader would. A mention of a FOLDER reaches the docs directly in it (`.claude/rules/`, `docs/`),
//! not the whole subtree below: naming `src/` is not a pointer to every README under it.

use super::File;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// The docs nothing loaded at the start of a session leads to, in path order. Empty when the tree has no such
/// starting doc at all - a tree without a CLAUDE.md has no reader to be orphaned from.
pub fn orphan_docs(files: &BTreeMap<String, File>) -> Vec<String> {
    let docs: BTreeSet<&str> = files.values().filter(|f| f.language == "markdown").map(|f| f.rel.as_str()).collect();
    let mut reached: BTreeSet<&str> = docs.iter().copied().filter(|rel| starts_a_session(rel)).collect();
    if reached.is_empty() {
        return Vec::new();
    }
    let mut queue: VecDeque<&str> = reached.iter().copied().collect();
    while let Some(doc) = queue.pop_front() {
        let Some(file) = files.get(doc) else { continue };
        for mention in &file.mentions {
            let folder = mention.trim_end_matches('/');
            let named = docs.iter().copied().filter(|d| *d == mention.as_str() || parent(d) == folder);
            for next in named {
                if reached.insert(next) {
                    queue.push_back(next);
                }
            }
        }
    }
    docs.into_iter().filter(|d| !reached.contains(d)).map(String::from).collect()
}

/// Loaded by the harness on its own: a CLAUDE.md or AGENTS.md in any folder, anything under a `.claude/` folder.
fn starts_a_session(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').collect();
    let name = parts.last().copied().unwrap_or("");
    name.eq_ignore_ascii_case("CLAUDE.md") || name.eq_ignore_ascii_case("AGENTS.md") || parts.contains(&".claude")
}

fn parent(rel: &str) -> &str {
    rel.rfind('/').map_or("", |at| &rel[..at])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(rel: &str, mentions: &[&str]) -> (String, File) {
        let file = File {
            rel: rel.into(),
            language: "markdown".into(),
            mentions: mentions.iter().map(|m| m.to_string()).collect(),
            ..Default::default()
        };
        (rel.into(), file)
    }

    #[test]
    fn a_doc_is_reached_by_a_chain_of_mentions_or_its_folder_and_the_rest_is_orphaned() {
        let files: BTreeMap<String, File> = [
            doc("CLAUDE.md", &["README.md", ".claude/rules/"]),
            doc("README.md", &["docs/a.md"]),
            doc("docs/a.md", &["docs/b.md"]),
            doc("docs/b.md", &[]),
            doc("docs/lost.md", &[]),
            doc(".claude/rules/x.md", &[]),
            doc(".claude/agents/y.md", &["docs/agent-only.md"]),
            doc("docs/agent-only.md", &[]),
            doc("docs/deep/c.md", &[]),
        ]
        .into_iter()
        .collect();
        // `docs/` is never named as a folder, and naming `docs/a.md` reaches no sibling.
        assert_eq!(orphan_docs(&files), vec!["docs/deep/c.md", "docs/lost.md"]);
    }

    #[test]
    fn a_tree_with_no_starting_doc_has_no_orphans() {
        let files: BTreeMap<String, File> = [doc("README.md", &[]), doc("docs/a.md", &[])].into_iter().collect();
        assert!(orphan_docs(&files).is_empty());
    }
}
