//! `--facts-review`: A MODEL READS THE `--facts-suggest` CANDIDATES (`crate::llm`). A candidate is two sets sharing
//! values, and sharing values is not meaning the same thing - `1, 2, 3` are in every enum. The model is asked, for
//! every candidate at once, whether the document's part really lists what the code set holds, and answers each with a
//! verdict and a reason. Its word is written BESIDE the candidates, never into the config: each candidate gets its
//! `review` in `structuregate.facts.suggest.json`, and the accepted ones are a ready `facts`/`links` fragment in
//! `structuregate.facts.reviewed.json`. A person copies it in - the map run stays what the config says.

use crate::llm::{Ask, Llm};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Path;

pub const FILE_NAME: &str = "structuregate.facts.reviewed.json";

const SYSTEM: &str = "You review candidate links between a DOCUMENT and a CODE MAP. Each candidate pairs one part of the document \
(a table column, the code spans or the routes of a section) with one set of values the code holds (a seeded table column, an enum's \
members, a class's constants, the HTTP endpoints, a type's properties, a SQL table's columns). They were paired only because they \
share values. ACCEPT a candidate when the document's part is a list of the same thing the code set holds. GAPS ARE EXPECTED: a value \
only one side has is exactly what the check that follows will report, so unmatched values are never a reason to reject - judge \
whether the two sides NAME THE SAME CONCEPT, not whether they agree. REJECT it only when the overlap is a coincidence: small numbers or common words that \
any list shares, a column of a different concept that happens to reuse some values, or a pair another candidate covers better. \
When a candidate carries `section_endpoints`, the document's part sits under a section naming those HTTP endpoints, and `endpoint_type` says whether the code set is a type such an endpoint takes (request) or returns (response), as the compiler resolved it; a type's properties with `endpoint_type` null are another class that only shares the names - prefer the one the endpoint uses. Judge only from what you are given. Answer every candidate by its number, with a reason of one sentence.";

/// The verdicts written beside the candidates, and the lines that say them; an error when no model answered.
pub fn run(llm: &dyn Llm, suggested: &Path) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(suggested).map_err(|e| format!("{} could not be read - {e}", suggested.display()))?;
    let mut doc: Value = serde_json::from_str(&text).map_err(|e| format!("{} is no JSON - {e}", suggested.display()))?;
    let candidates = doc["candidates"].as_array().cloned().unwrap_or_default();
    if candidates.is_empty() {
        return Ok(vec!["facts review: no candidate to review".into()]);
    }
    let answer = llm.ask_json(&ask(&candidates)).map_err(|e| e.to_string())?;
    let verdicts = answer["verdicts"].as_array().cloned().unwrap_or_default();
    let said: Vec<(bool, String)> = (1..=candidates.len())
        .map(|n| {
            let verdict = verdicts.iter().find(|v| v["n"].as_u64() == Some(n as u64));
            (verdict.is_some_and(|v| v["accept"] == true), verdict.and_then(|v| v["reason"].as_str()).unwrap_or("the model gave no verdict for it").to_string())
        })
        .collect();
    // ONE PART OF THE DOCUMENT ACCEPTED AGAINST SEVERAL SETS is no answer yet: each is kept, and the fragment says so.
    let mut targets: BTreeMap<&str, Vec<Value>> = BTreeMap::new();
    for (candidate, _) in candidates.iter().zip(&said).filter(|(_, (accept, _))| *accept) {
        targets.entry(candidate["found"].as_str().unwrap_or("")).or_default().push(candidate["link"]["to"].clone());
    }
    targets.retain(|_, to| to.len() > 1);
    let mut lines = Vec::new();
    let (mut facts, mut links) = (Vec::new(), Vec::new());
    for (i, (candidate, (accept, reason))) in candidates.iter().zip(&said).enumerate() {
        let found = candidate["found"].as_str().unwrap_or("");
        let ambiguous = *accept && targets.contains_key(found);
        let verdict = match (accept, ambiguous) {
            (true, true) => "ACCEPTED, AMBIGUOUS",
            (true, false) => "ACCEPTED",
            _ => "rejected",
        };
        lines.push(format!("  {found}: {verdict} - {reason}"));
        doc["candidates"][i]["review"] = json!({ "accept": accept, "reason": reason });
        if ambiguous {
            doc["candidates"][i]["review"]["ambiguous"] = json!(true);
        }
        if *accept {
            facts.push(candidate["facts"].clone());
            links.push(candidate["link"].clone());
        }
    }
    let folder = suggested.parent().unwrap_or(Path::new("."));
    let reviewed = folder.join(FILE_NAME);
    let mut fragment = json!({ "facts": facts, "links": links });
    if !targets.is_empty() {
        fragment["ambiguous"] = json!(targets.iter().map(|(found, to)| json!({ "found": found, "to": to })).collect::<Vec<_>>());
    }
    write(suggested, &doc)?;
    write(&reviewed, &fragment)?;
    lines.insert(0, format!("facts review: {} of {} accepted -> {}", facts.len(), candidates.len(), reviewed.display()));
    Ok(lines)
}

/// One question for every candidate: its number, what it pairs, its counts and a sample of each side.
fn ask(candidates: &[Value]) -> Ask {
    let listed: Vec<Value> = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut one = json!({
                "n": i + 1, "document": c["found"], "code": c["link"]["to"], "matched": c["matched"], "of": c["of"],
                "code_values": c["code_values"], "matched_values": c["matched_values"], "unmatched_document_values": c["unmatched"],
            });
            if let Some(endpoints) = c.get("section_endpoints") {
                one["section_endpoints"] = endpoints.clone();
                one["endpoint_type"] = c["endpoint_type"].clone();
            }
            one
        })
        .collect();
    let schema = json!({
        "type": "object", "required": ["verdicts"],
        "properties": { "verdicts": { "type": "array", "items": {
            "type": "object", "required": ["n", "accept", "reason"],
            "properties": { "n": { "type": "integer" }, "accept": { "type": "boolean" }, "reason": { "type": "string" } }
        } } }
    });
    Ask { system: SYSTEM.into(), message: serde_json::to_string_pretty(&json!({ "candidates": listed })).unwrap_or_default(), schema }
}

fn write(path: &Path, value: &Value) -> Result<(), String> {
    std::fs::write(path, serde_json::to_string_pretty(value).unwrap_or_default()).map_err(|e| format!("{} could not be written - {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::Failed;
    use std::cell::RefCell;

    /// A model that answers what it is given and keeps the question.
    struct Fake(Result<Value, Failed>, RefCell<Option<Ask>>);

    impl Llm for Fake {
        fn ask_json(&self, ask: &Ask) -> Result<Value, Failed> {
            *self.1.borrow_mut() = Some(ask.clone());
            self.0.clone()
        }
    }

    #[test]
    fn the_verdicts_go_beside_the_candidates_and_only_the_accepted_into_the_fragment() {
        let dir = std::env::temp_dir().join(format!("fbt-review-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let suggested = dir.join(super::super::suggest::FILE_NAME);
        let candidate = |found: &str, kind: &str| {
            json!({"found": found, "matched": 3, "of": 4, "code_values": 4, "matched_values": ["a"], "unmatched": ["b"],
                   "facts": {"name": found}, "link": {"facts": found, "to": {"kind": kind}}})
        };
        let mut typed = candidate("fields", "property");
        typed["section_endpoints"] = json!(["POST /api/items"]);
        typed["endpoint_type"] = json!("response");
        let mut other = candidate("fields", "property");
        other["link"]["to"]["symbol"] = json!("Demo.Other");
        let all = [candidate("routes", "endpoint"), candidate("codes", "enum"), candidate("rest", "seed"), typed, other];
        std::fs::write(&suggested, json!({"map": "m", "candidates": all}).to_string()).unwrap();
        let said = json!({"verdicts": [{"n": 1, "accept": true, "reason": "the spec lists the endpoints"}, {"n": 2, "accept": false, "reason": "small numbers"},
                                       {"n": 4, "accept": true, "reason": "the response"}, {"n": 5, "accept": true, "reason": "same names"}]});
        let fake = Fake(Ok(said), RefCell::new(None));
        let lines = run(&fake, &suggested).unwrap();
        assert_eq!(lines[1], "  routes: ACCEPTED - the spec lists the endpoints");
        assert_eq!(lines[3], "  rest: rejected - the model gave no verdict for it");
        let fragment: Value = serde_json::from_str(&std::fs::read_to_string(dir.join(FILE_NAME)).unwrap()).unwrap();
        assert_eq!(lines[4], "  fields: ACCEPTED, AMBIGUOUS - the response");
        assert_eq!(fragment["links"][0], json!({"facts": "routes", "to": {"kind": "endpoint"}}));
        assert_eq!(fragment["facts"].as_array().unwrap().len(), 3);
        assert_eq!(fragment["ambiguous"], json!([{"found": "fields", "to": [{"kind": "property"}, {"kind": "property", "symbol": "Demo.Other"}]}]));
        let kept: Value = serde_json::from_str(&std::fs::read_to_string(&suggested).unwrap()).unwrap();
        assert_eq!(kept["candidates"][1]["review"], json!({"accept": false, "reason": "small numbers"}));
        let asked = fake.1.borrow().clone().unwrap();
        let message: Value = serde_json::from_str(&asked.message).unwrap();
        assert_eq!(message["candidates"][2]["n"], 3);
        assert_eq!(message["candidates"][0]["unmatched_document_values"], json!(["b"]));
        assert_eq!((&message["candidates"][3]["section_endpoints"], &message["candidates"][3]["endpoint_type"]), (&json!(["POST /api/items"]), &json!("response")));
        assert!(message["candidates"][0].get("section_endpoints").is_none());
        assert_eq!(kept["candidates"][4]["review"]["ambiguous"], true);
        let down = Fake(Err(Failed::Unavailable("quota".into())), RefCell::new(None));
        assert_eq!(run(&down, &suggested).unwrap_err(), "no model could be reached - quota");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
