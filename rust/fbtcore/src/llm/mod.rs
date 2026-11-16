//! THE AI INTERFACE: a question with a JSON schema in, the answer as JSON out - and nothing about who answers. The map
//! itself never asks: a run is what its tree and its config say, every time. A feature that wants a model's judgement
//! (`facts/review.rs`: which `--facts-suggest` candidates are real) takes a `&dyn Llm` and writes what it answered to a
//! file of its own, for a person to accept.
//!
//! WHO ANSWERS IS CONFIGURED, OFF UNLESS NAMED (`connect`): `STRUCTUREGATE_LLM` is a program, `STRUCTUREGATE_LLM_ARGS`
//! its arguments - one process per question, JSON over stdin and stdout (`cli.rs`), so the vendor, the model, the
//! credentials and the retries are that program's configuration, never this exe's. A question may offer TOOLS
//! (`Ask::tools`, read-only, e.g. a SQL query over the map) - the program runs them; the answer says how often. Another backend is one more `Llm`. Nothing is linked in: no SDK, no socket.

mod cli;

use serde_json::Value;

pub use cli::Cli;

pub const ENV: &str = "STRUCTUREGATE_LLM";
pub const ARGS_ENV: &str = "STRUCTUREGATE_LLM_ARGS";

/// One question: the instructions, the material, and the shape the answer must have.
#[derive(Debug, Clone)]
pub struct Ask {
    pub system: String,
    pub message: String,
    /// A JSON schema the answer is checked against (by the program that answers).
    pub schema: Value,
    /// Read-only tools the model may call while it answers, each a JSON object the program reads
    /// (`{"type": "sqlite_query", "path", "name", "description", "max_calls"}`); none, and none is sent.
    pub tools: Vec<Value>,
}

/// An answer: its data, and how many tool calls the program says the model made (None when it says nothing).
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub data: Value,
    pub tool_calls: Option<u64>,
}

/// Why there is no answer, by what the caller can do about it.
#[derive(Debug, Clone, PartialEq)]
pub enum Failed {
    /// No backend reached: not installed, not logged in, out of quota.
    Unavailable(String),
    /// An answer came, and it is not what the schema asks.
    Invalid(String),
    /// No answer in time.
    Timeout(String),
    /// The backend refused this question (too long, ..).
    Rejected(String),
    /// Anything else, said as it was said.
    Other(String),
}

impl std::fmt::Display for Failed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (what, why) = match self {
            Failed::Unavailable(w) => ("no model could be reached", w),
            Failed::Invalid(w) => ("the answer did not have the shape asked for", w),
            Failed::Timeout(w) => ("no answer in time", w),
            Failed::Rejected(w) => ("the question was refused", w),
            Failed::Other(w) => ("the question failed", w),
        };
        write!(f, "{what} - {why}")
    }
}

/// Whoever answers a question.
pub trait Llm {
    /// The answer as JSON, or why there is none. Never panics, never prompts.
    fn ask_json(&self, ask: &Ask) -> Result<Answer, Failed>;
}

/// The configured backend, None when the environment names none, or why the one it names cannot be used.
pub fn connect() -> Option<Cli> {
    from(&std::env::var(ENV).unwrap_or_default(), &std::env::var(ARGS_ENV).unwrap_or_default())
}

/// `STRUCTUREGATE_LLM` and `STRUCTUREGATE_LLM_ARGS` as a backend: empty, `0`, `off`, `false` or `none` is none;
/// anything else is the program - a path, or a name on PATH - and its arguments split at whitespace (a path with a
/// space in it goes in the program, not the arguments).
fn from(named: &str, args: &str) -> Option<Cli> {
    let named = named.trim();
    if matches!(named.to_lowercase().as_str(), "" | "0" | "off" | "false" | "none") {
        return None;
    }
    Some(Cli { exe: std::path::PathBuf::from(named), args: args.split_whitespace().map(String::from).collect() })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_model_unless_one_is_named_and_its_arguments_split_at_whitespace() {
        for off in ["", " ", "0", "OFF", "false", "none"] {
            assert!(from(off, "--x").is_none(), "{off:?}");
        }
        let cli = from(" /opt/my llm/answer ", " judge  --json -  ").unwrap();
        assert_eq!(cli.exe, std::path::PathBuf::from("/opt/my llm/answer"));
        assert_eq!(cli.args, ["judge", "--json", "-"]);
        assert!(from("answer", "").unwrap().args.is_empty());
    }
}
