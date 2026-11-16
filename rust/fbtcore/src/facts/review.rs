//! `--facts-review`: A MODEL READS THE `--facts-suggest` CANDIDATES (`crate::llm`). A candidate is two sets sharing
//! values, and sharing values is not meaning the same thing - `1, 2, 3` are in every enum. The model is asked, in
//! batches with the map as a tool (`asking.rs`), whether each document part really lists what the code set holds, and
//! answers each with a verdict and a reason; a batch that fails leaves its candidates unreviewed and says so. Its word is written BESIDE the candidates, never into the config: each candidate gets its
//! `review` in `structuregate.facts.suggest.json`, and the accepted ones are a ready `facts`/`links` fragment in
//! `structuregate.facts.reviewed.json`. A person copies it in - the map run stays what the config says.

use super::asking;
use crate::llm::Llm;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

pub const FILE_NAME: &str = "structuregate.facts.reviewed.json";

/// The verdicts written beside the candidates, and the lines that say them; an error when no model answered.
pub fn run(llm: &dyn Llm, suggested: &Path) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(suggested).map_err(|e| format!("{} could not be read - {e}", suggested.display()))?;
    let mut doc: Value = serde_json::from_str(&text).map_err(|e| format!("{} is no JSON - {e}", suggested.display()))?;
    let candidates = doc["candidates"].as_array().cloned().unwrap_or_default();
    if candidates.is_empty() {
        return Ok(vec!["facts review: no candidate to review".into()]);
    }
    let tool = asking::map_tool(doc["map"].as_str().unwrap_or(""), &std::env::var(asking::TOOLS_ENV).unwrap_or_default());
    let batches = asking::in_batches(&candidates);
    // EACH BATCH ON ITS OWN: one that fails leaves its candidates unreviewed, the others still count.
    let mut verdicts: HashMap<u64, Value> = HashMap::new();
    let mut unreviewed: HashMap<usize, String> = HashMap::new();
    let (mut failed, mut calls, mut counted) = (Vec::new(), 0u64, true);
    for (b, batch) in batches.iter().enumerate() {
        match llm.ask_json(&asking::ask(&candidates, batch, tool.as_ref())) {
            Ok(answer) => {
                calls += answer.tool_calls.unwrap_or(0);
                counted &= answer.tool_calls.is_some();
                let ours = |n: u64| batch.iter().any(|&i| i as u64 + 1 == n);
                for v in answer.data["verdicts"].as_array().into_iter().flatten() {
                    if let Some(n) = v["n"].as_u64().filter(|n| ours(*n)) {
                        verdicts.entry(n).or_insert_with(|| v.clone());
                    }
                }
            }
            Err(why) => {
                let why = why.to_string();
                batch.iter().for_each(|&i| drop(unreviewed.insert(i, why.clone())));
                failed.push(format!("  batch {} of {} NOT REVIEWED ({} candidate(s)) - {why}", b + 1, batches.len(), batch.len()));
            }
        }
    }
    if failed.len() == batches.len() {
        return Err(unreviewed.into_values().next().unwrap_or_default());
    }
    let said: Vec<(bool, String)> = (1..=candidates.len())
        .map(|n| {
            if let Some(why) = unreviewed.get(&(n - 1)) {
                return (false, why.clone());
            }
            let verdict = verdicts.get(&(n as u64));
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
        let verdict = match (accept, ambiguous, unreviewed.contains_key(&i)) {
            (_, _, true) => "NOT REVIEWED",
            (true, true, _) => "ACCEPTED, AMBIGUOUS",
            (true, false, _) => "ACCEPTED",
            _ => "rejected",
        };
        lines.push(format!("  {found}: {verdict} - {reason}"));
        doc["candidates"][i]["review"] = json!({ "accept": accept, "reason": reason });
        if unreviewed.contains_key(&i) {
            doc["candidates"][i]["review"] = json!({ "reviewed": false, "reason": reason });
        }
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
    let used = match (&tool, counted) {
        (None, _) => "no map tool".to_string(),
        (Some(_), true) => format!("{} {} call(s)", asking::TOOL_NAME, calls),
        (Some(_), false) => format!("{} offered, calls not reported", asking::TOOL_NAME),
    };
    lines.splice(0..0, failed);
    lines.insert(0, format!("facts review: {} of {} accepted -> {} ({} batch(es), {used})", facts.len(), candidates.len(), reviewed.display(), batches.len()));
    Ok(lines)
}

fn write(path: &Path, value: &Value) -> Result<(), String> {
    std::fs::write(path, serde_json::to_string_pretty(value).unwrap_or_default()).map_err(|e| format!("{} could not be written - {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{Answer, Ask, Failed};
    use std::cell::RefCell;

    /// A model that gives its answers in turn (the last one again after that) and keeps every question.
    struct Fake(Vec<Result<Answer, Failed>>, RefCell<Vec<Ask>>);

    impl Fake {
        fn new(answers: Vec<Result<Value, Failed>>, calls: Option<u64>) -> Fake {
            Fake(answers.into_iter().map(|a| a.map(|data| Answer { data, tool_calls: calls })).collect(), RefCell::new(vec![]))
        }
    }

    impl Llm for Fake {
        fn ask_json(&self, ask: &Ask) -> Result<Answer, Failed> {
            let mut asked = self.1.borrow_mut();
            asked.push(ask.clone());
            self.0[(asked.len() - 1).min(self.0.len() - 1)].clone()
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
        let fake = Fake::new(vec![Ok(said)], None);
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
        assert!(lines[0].ends_with("(1 batch(es), no map tool)"), "{}", lines[0]);
        let asked = fake.1.borrow()[0].clone();
        let message: Value = serde_json::from_str(&asked.message).unwrap();
        assert_eq!(message["candidates"][2]["n"], 3);
        assert_eq!(message["candidates"][0]["unmatched_document_values"], json!(["b"]));
        assert_eq!((&message["candidates"][3]["section_endpoints"], &message["candidates"][3]["endpoint_type"]), (&json!(["POST /api/items"]), &json!("response")));
        assert!(message["candidates"][0].get("section_endpoints").is_none());
        assert_eq!(kept["candidates"][4]["review"]["ambiguous"], true);
        let down = Fake::new(vec![Err(Failed::Unavailable("quota".into()))], None);
        assert_eq!(run(&down, &suggested).unwrap_err(), "no model could be reached - quota");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn each_batch_is_asked_with_the_map_as_a_tool_and_a_failed_batch_leaves_only_its_candidates_unreviewed() {
        let dir = std::env::temp_dir().join(format!("fbt-review-batch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let map = dir.join("map.sqlite");
        rusqlite::Connection::open(&map).unwrap().execute_batch("CREATE TABLE members (id, kind, class_symbol, name);").unwrap();
        let suggested = dir.join(super::super::suggest::FILE_NAME);
        // TWO PARTS TOO LARGE FOR ONE BATCH.
        let big = |found: &str| json!({"found": found, "matched": 1, "of": 1, "code_values": 1, "matched_values": ["v".repeat(40_000)], "unmatched": [],
                                       "facts": {"name": found}, "link": {"facts": found, "to": {"kind": "enum"}}});
        std::fs::write(&suggested, json!({"map": map.to_str(), "candidates": [big("a"), big("b")]}).to_string()).unwrap();
        let yes = |n: u64| Ok(json!({"verdicts": [{"n": n, "accept": true, "reason": "the map shows it"}, {"n": 3 - n, "accept": false, "reason": "not this batch's"}]}));
        let fake = Fake::new(vec![yes(1), Err(Failed::Timeout("slow".into()))], Some(3));
        let lines = run(&fake, &suggested).unwrap();
        assert!(lines[0].starts_with("facts review: 1 of 2 accepted") && lines[0].ends_with("(2 batch(es), map_query 3 call(s))"), "{}", lines[0]);
        assert_eq!(lines[1], "  batch 2 of 2 NOT REVIEWED (1 candidate(s)) - no answer in time - slow");
        assert_eq!(lines[3], "  b: NOT REVIEWED - no answer in time - slow");
        let kept: Value = serde_json::from_str(&std::fs::read_to_string(&suggested).unwrap()).unwrap();
        assert_eq!(kept["candidates"][1]["review"], json!({"reviewed": false, "reason": "no answer in time - slow"}));
        let asked = fake.1.borrow();
        assert_eq!(asked.len(), 2);
        assert_eq!(asked[0].tools[0]["path"].as_str(), map.to_str());
        assert!(asked[1].message.contains("\"n\": 2") && !asked[1].message.contains("\"n\": 1"));
        drop(asked);
        let silent = Fake::new(vec![yes(1), yes(2)], None);
        let lines = run(&silent, &suggested).unwrap();
        assert!(lines[0].ends_with("(2 batch(es), map_query offered, calls not reported)"), "{}", lines[0]);
        assert_eq!(lines[2], "  b: ACCEPTED - the map shows it");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
