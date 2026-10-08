//! ONE DOC, WALKED. `pulldown-cmark` hands back events with byte ranges; this turns them into what
//! the map asks of a doc - its title, its frontmatter, and every place it NAMES something: an inline
//! code span, a link target, and each word of a command block.
//!
//! WHAT IS NOT A NAME. Prose is not read for paths - a word in a sentence is not a claim a reader
//! follows - and a fenced block in a language (`json`, `csharp`, `text`) is example content, not a
//! command anyone runs. Only a SHELL block is split into words, because that is the block a reader
//! pastes, and a path in it that is not there fails the first time it is followed.

use crate::rsmap::lines::LineStarts;
use pulldown_cmark::{CodeBlockKind, Event, MetadataBlockKind, Options, Parser, Tag, TagEnd};
use yaml_rust2::YamlLoader;

/// What a doc says, before anything is resolved against the disk.
#[derive(Default)]
pub struct Doc {
    pub title: String,
    pub name: Option<String>,
    pub description: Option<String>,
    pub named: Vec<Named>,
}

/// One thing a doc names, where it names it, and how.
pub struct Named {
    pub text: String,
    pub line: usize,
    /// `code` (an inline span), `link` (a link target) or `command` (a word of a shell block).
    pub kind: &'static str,
    /// The folder a `cd` earlier in the same block moved to - where a reader stands when they run it.
    pub cd: Option<String>,
}

/// The fence infos that are a command a reader runs. NOT an unlabelled block: across the trees it was tried on
/// that is as often a folder tree (`Components/` over `App.razor`), a message or a diagram as a command,
/// and read as words it named `Read/Write` and half of `Shared Files/` as paths.
const SHELL: [&str; 9] = ["bash", "sh", "shell", "console", "powershell", "pwsh", "ps1", "cmd", "bat"];

pub fn walk(text: &str) -> Doc {
    let starts = LineStarts::new(text);
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH;
    let mut doc = Doc::default();
    let mut heading: Option<String> = None;
    let mut front = false;
    let mut shell = false;
    let mut cd: Option<String> = None;

    for (event, range) in Parser::new_ext(text, options).into_offset_iter() {
        let line = starts.line_of(range.start);
        match event {
            Event::Start(Tag::MetadataBlock(MetadataBlockKind::YamlStyle)) => front = true,
            Event::End(TagEnd::MetadataBlock(_)) => front = false,
            Event::Text(body) if front => frontmatter(&mut doc, &body),
            Event::Start(Tag::Heading { .. }) if doc.title.is_empty() => heading = Some(String::new()),
            Event::End(TagEnd::Heading(_)) => {
                if let Some(title) = heading.take() {
                    doc.title = title.trim().to_string();
                }
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                doc.named.push(Named { text: dest_url.to_string(), line, kind: "link", cd: None });
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                shell = match kind {
                    CodeBlockKind::Fenced(info) => SHELL.contains(&info.split_whitespace().next().unwrap_or("")),
                    CodeBlockKind::Indented => false,
                };
                cd = None;
            }
            Event::End(TagEnd::CodeBlock) => shell = false,
            Event::Text(body) if shell => commands(&mut doc, &body, line, &mut cd),
            Event::Code(span) => {
                if let Some(title) = heading.as_mut() {
                    title.push_str(&span);
                }
                doc.named.push(Named { text: span.to_string(), line, kind: "code", cd: None });
            }
            Event::Text(body) => {
                if let Some(title) = heading.as_mut() {
                    title.push_str(&body);
                }
            }
            _ => {}
        }
    }
    doc
}

/// A skill's or an agent's frontmatter: `name` is what the doc is CALLED by whoever loads it.
fn frontmatter(doc: &mut Doc, body: &str) {
    let Ok(documents) = YamlLoader::load_from_str(body) else { return };
    let Some(first) = documents.first() else { return };
    doc.name = first["name"].as_str().map(str::to_string);
    doc.description = first["description"].as_str().map(str::to_string);
}

/// Every word of a shell block, one line at a time. A `#` word ends the line (a comment), and `cd X`
/// moves where the REST of the block runs - `cd src/app` above `python .claude/hooks/x.py` makes
/// that path relative to `src/app`, which is how the reader will run it.
fn commands(doc: &mut Doc, body: &str, first_line: usize, cd: &mut Option<String>) {
    for (offset, row) in body.lines().enumerate() {
        let words = words(row);
        let mut at = 0;
        while at < words.len() {
            let word = words[at].trim_matches(|c| c == ';' || c == ',');
            if word.starts_with('#') {
                break;
            }
            if word == "cd" && at + 1 < words.len() {
                // THE FOLDER ITSELF IS A CLAIM - `cd src/app` in a tree without one is the first
                // line a reader runs and the first to fail - resolved from where the block started.
                doc.named.push(Named { text: words[at + 1].clone(), line: first_line + offset, kind: "command", cd: cd.clone() });
                *cd = Some(words[at + 1].clone());
                at += 2;
                continue;
            }
            doc.named.push(Named { text: word.to_string(), line: first_line + offset, kind: "command", cd: cd.clone() });
            at += 1;
        }
    }
}

/// A command line's words, a QUOTED one kept whole with its spaces: `"C:\Program Files (x86)\x"` is one
/// word, and split at its spaces it read as a path `C:\Program` that is not there.
fn words(row: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote: Option<char> = None;
    for c in row.chars() {
        match (quote, c) {
            (Some(open), _) if c == open => quote = None,
            (Some(_), _) => word.push(c),
            (None, '"' | '\'') => quote = Some(c),
            (None, _) if c.is_whitespace() => {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
            }
            (None, _) => word.push(c),
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}
