//! THE DOC RULES' WORDS: what a too-long doc is told to do, and which local links lead nowhere.
//!
//! NAME THE SECTIONS. "Too long" without them invites a TRIM, and trimming a doc deletes the measured
//! reasons nobody can re-derive. The fix is to MOVE a section out and REFERENCE it - which is also why the
//! gate fails on a link to a doc that does not exist. EVERY SPLIT STAYS AT THE SAME FOLDER LEVEL, a
//! sibling file, never a new subfolder - unless the folder is full.

use std::path::{Component, Path, PathBuf};

/// A doc loaded into a session without anyone asking - a CLAUDE.md or AGENTS.md, an agent, a skill, a rule (loaded
/// when its files are touched), a command - so every line of it is paid for on every session. The gate's `--doc-scope context` and the map's DOC-MISSING agree on it.
pub(crate) fn context_doc(rel: &str) -> bool {
    let parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty()).collect();
    let Some(last) = parts.last() else { return false };
    last.eq_ignore_ascii_case("CLAUDE.md")
        || last.eq_ignore_ascii_case("AGENTS.md")
        || parts.contains(&".claude") && ["agents", "skills", "rules", "commands"].iter().any(|f| parts.contains(f))
}

/// Whether the doc's folder can still TAKE the sibling a split would create - counted over sources and
/// docs together. Not a violation of its own: it only redirects WHERE the split goes.
pub struct Crowding {
    pub files: i64,
    pub limit: i64,
}

pub fn too_long(path: &str, rel: &str, lines: i64, text: &str, limit: i64, crowd: &Crowding) -> String {
    let mut sections: Vec<(String, i64)> = Vec::new();
    let mut title = "(before the first heading)".to_string();
    let mut count = 0;
    for raw in text.split('\n') {
        let line = raw.trim();
        if line.starts_with('#') {
            if count > 0 {
                sections.push((title.clone(), count));
            }
            title = line.to_string();
            count = 0;
        } else if !line.is_empty() {
            count += 1;
        }
    }
    if count > 0 {
        sections.push((title, count));
    }
    // STABLE, so equal sections keep the order the doc has them in.
    sections.sort_by(|a, b| b.1.cmp(&a.1));
    let largest: Vec<String> = sections.iter().take(5).map(|(t, n)| format!("      {n:>4} lines  {t}")).collect();
    format!(
        "{rel}: {lines} non-blank lines (limit {limit}). SPLIT, do not trim — {} Largest sections:\n{}",
        remedy(path, rel, crowd),
        largest.join("\n")
    )
}

/// What to actually DO, chosen by what the file is. Every form is the same move - the detail leaves the
/// always-loaded file and stays reachable by a path.
fn remedy(path: &str, rel: &str, crowd: &Crowding) -> String {
    let name = Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    // The REPO-relative folder: a message printing an absolute path names a location the reader has to
    // translate back before it means anything.
    let here = parent(rel);
    let here = if here.is_empty() { ".".to_string() } else { here };
    let dir = parent(path);
    let stem = Path::new(path).file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let lower_dir = dir.to_ascii_lowercase();
    // THE ONE EXCEPTION TO SAME-LEVEL, measured rather than assumed: a full folder cannot take a sibling.
    if crowd.files + 1 > crowd.limit {
        return format!(
            "`{here}/` already holds {} measured files against a limit of {}, so it cannot take a sibling: move \
             the section into `{stem}/<topic>.md` and link it. A SUBFOLDER is right here ONLY because the folder \
             is full — everywhere else the split stays at the same level. Never trim; the detail leaves the \
             always-loaded file and stays reachable by a path.",
            crowd.files, crowd.limit
        );
    }
    if lower_dir.contains("/.claude/agents") {
        return format!(
            "this is an AGENT definition. Keep the frontmatter and the `description` intact — that is what routes \
             work to it — and move a body section into `{stem}-<topic>.md` BESIDE it in `{here}/`, referenced from \
             the body by its relative path so the agent reads it only when the task needs it. Procedures, non-bug \
             lists and report formats are the usual first to move. THE SPLIT FILE MUST HAVE NO FRONTMATTER: this \
             folder is the agent DISCOVERY path, and a `.md` here that carries `name`/`description` is a second \
             agent, not a reference. The `<agent>-<topic>` prefix keeps it visibly subordinate to its owner."
        );
    }
    if name.eq_ignore_ascii_case("SKILL.md") || lower_dir.contains("/.claude/skills") {
        return format!(
            "this is a SKILL. Keep the frontmatter and the overview, and move the detail into `<topic>.md` BESIDE \
             it in `{here}/` — the skill's own folder, at the SAME level as SKILL.md — named from the body by its \
             relative path. A skill is meant to disclose progressively: the body is what is always loaded, a \
             reference is read on demand. Only SKILL.md is loaded from this folder, so a sibling costs nothing \
             until read."
        );
    }
    if name.eq_ignore_ascii_case("CLAUDE.md") || name.eq_ignore_ascii_case("AGENTS.md") {
        return format!(
            "this is a CONTEXT file loaded every session. Move a section into `<topic>.md` BESIDE it in `{here}/` \
             — the SAME folder level, not a `docs/` subfolder — and LINK it. A link is followed when it is the \
             answer, while an `@import` costs the same context as leaving the text here. Guidance about another \
             directory belongs in a CLAUDE.md inside THAT directory."
        );
    }
    "move a section into a `<topic>.md` beside this file, in the same folder, and link it.".to_string()
}

/// The folder part with `/`, or "" at the top.
fn parent(path: &str) -> String {
    let path = path.replace('\\', "/");
    path.rfind('/').map_or(String::new(), |at| path[..at].to_string())
}

/// Every local link to a `.md` file that is not there. PARSED by `pulldown-cmark`, the parser the Markdown half uses:
/// an inline link and a reference-style one (`[x][ref]` with `[ref]: path.md`) alike, and nothing inside a code
/// block or a code span - an example of a link is not one. `#part` is not part of the file. A URL is not followed.
pub fn broken_links(rel: &str, abs: &str, text: &str) -> Vec<String> {
    use pulldown_cmark::{Event, Options, Parser, Tag};
    let folder = Path::new(abs).parent().map(Path::to_path_buf).unwrap_or_default();
    let mut broken = Vec::new();
    for event in Parser::new_ext(text, Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS) {
        let Event::Start(Tag::Link { dest_url, .. }) = event else { continue };
        let target = dest_url.split(['#', '?']).next().unwrap_or("").trim();
        if target.is_empty() || target.contains("://") || target.starts_with("mailto:") || !target.to_ascii_lowercase().ends_with(".md") {
            continue;
        }
        if !lexical(&folder.join(target)).is_file() {
            broken.push(format!("{rel}: links to {target}, which does not exist"));
        }
    }
    broken
}

fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_largest_sections_are_named_and_a_full_folder_sends_the_split_down_a_level() {
        let text = "# A\none\ntwo\n# B\nthree\n";
        let said = too_long("x/CLAUDE.md", "CLAUDE.md", 3, text, 2, &Crowding { files: 14, limit: 14 });
        assert!(said.contains("         2 lines  # A\n         1 lines  # B"), "{said}");
        assert!(said.contains("already holds 14 measured files"), "{said}");
    }

    #[test]
    fn a_link_that_spans_lines_or_is_a_url_is_not_followed() {
        let text = "[a](gone.md) [b](https://x.io/a.md) [c](half\n.md)";
        assert_eq!(broken_links("d.md", "C:/nowhere/d.md", text), ["d.md: links to gone.md, which does not exist"]);
    }

    #[test]
    fn a_rule_and_a_command_are_context_docs_as_agents_and_skills_are() {
        for doc in ["CLAUDE.md", "x/AGENTS.md", ".claude/agents/a.md", ".claude/skills/s/SKILL.md", ".claude/rules/r.md", ".claude/commands/c.md"] {
            assert!(context_doc(doc), "{doc}");
        }
        for doc in ["README.md", "docs/rules/r.md", ".claude/notes.md"] {
            assert!(!context_doc(doc), "{doc}");
        }
    }

    #[test]
    fn a_reference_link_is_followed_and_an_example_in_code_is_not() {
        let text = "See [the guide][g] and [part](gone.md#part).\n\n[g]: ref-gone.md\n\n```md\n[x](in-a-block.md)\n```\n\n`[y](in-a-span.md)`\n";
        assert_eq!(
            broken_links("d.md", "C:/nowhere/d.md", text),
            ["d.md: links to ref-gone.md, which does not exist", "d.md: links to gone.md, which does not exist"]
        );
    }
}
