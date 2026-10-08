//! `reader.rs` and `table.rs` over the two snapshot shapes: a consumer's Markdown export (an empty GFM header, the column's
//! name in the first body row, the emphasis escaped, one list split over tables under bold paragraphs) and the
//! same document as the Docs API keeps it.
use super::super::config::Column;
use super::super::table;
use super::*;
use serde_json::json;

const EXPORTED: &str = "## **Product codes for order export**\n\n**Hardware line**\n\n|  |  |\n| :-: | :-: |\n\
| \\*\\*Product code\\*\\* |   |\n| \\*\\*7\\*\\* | Alpha laptop |\n| \\*\\*8\\*\\* | Alpha phone |\n\n\
**Software line**\n\n|  |  |\n| :-: | :-: |\n| \\*\\*Product code\\*\\* |   |\n| \\*\\*20\\*\\* | Beta suite |\n\n\
## Status\n\n| Key | Label |\n|---|---|\n| 1 | Active |\n";

fn keys(rows: &[table::Row]) -> Vec<(String, String, String)> {
    rows.iter().map(|r| (r.subsection.clone(), r.key.clone(), r.label.clone())).collect()
}

#[test]
fn an_escaped_list_split_over_tables_is_one_list_with_its_subsections() {
    let blocks = from_markdown(EXPORTED);
    let found = section(&blocks, "Product codes for order export", 0).unwrap();
    assert_eq!(found.len(), 2);
    let rows = table::rows(&found, &Column::Name("product code".into()), &Some(Column::At(2))).unwrap();
    assert_eq!(
        keys(&rows),
        vec![
            ("Hardware line".into(), "7".into(), "Alpha laptop".into()),
            ("Hardware line".into(), "8".into(), "Alpha phone".into()),
            ("Software line".into(), "20".into(), "Beta suite".into()),
        ]
    );
    assert_eq!(rows[0].at, 8);
    assert_eq!(rows[0].cells["Product code"], "7");
    assert_eq!(rows[0].cells["2"], "Alpha laptop");
}

#[test]
fn a_table_that_names_no_column_continues_the_one_before() {
    let text = "# A\n\n| Key | Label |\n|---|---|\n| 1 | One |\n\n| | |\n|---|---|\n| 2 | Two |\n";
    let blocks = from_markdown(text);
    let rows = table::rows(&section(&blocks, "A", 0).unwrap(), &Column::Name("Key".into()), &Some(Column::Name("Label".into()))).unwrap();
    assert_eq!(keys(&rows), vec![("".into(), "1".into(), "One".into()), ("".into(), "2".into(), "Two".into())]);
    let none = table::rows(&section(&blocks, "A", 0).unwrap(), &Column::Name("Code".into()), &None);
    assert!(none.unwrap_err().contains("no row of the first table holds a cell `Code`"));
}

#[test]
fn a_section_is_found_and_ends_at_the_next_heading_of_its_level() {
    let blocks = from_markdown(EXPORTED);
    assert_eq!(section(&blocks, "Status", 0).unwrap().len(), 1);
    assert_eq!(section(&blocks, "order export", 2).unwrap()[0].subsection, "Software line");
    assert!(section(&blocks, "order export", 3).unwrap_err().contains("has 2 table(s), not 3"));
    assert!(section(&blocks, "Nothing", 0).unwrap_err().contains("no heading"));
    let rows = table::rows(&section(&blocks, "Status", 1).unwrap(), &Column::At(1), &Some(Column::At(2))).unwrap();
    // By position, the GFM header row is a header, not a fact.
    assert_eq!(keys(&rows), vec![("".into(), "1".into(), "Active".into())]);
}

fn run(text: &str, bold: bool) -> Value {
    json!({"textRun": {"content": text, "textStyle": if bold { json!({"bold": true}) } else { json!({}) }}})
}

fn para(style: &str, runs: Vec<Value>) -> Value {
    json!({"paragraph": {"paragraphStyle": {"namedStyleType": style}, "elements": runs}})
}

fn row(at: u64, cells: &[(&str, bool)]) -> Value {
    let cells: Vec<Value> = cells.iter().map(|(t, b)| json!({"content": [para("NORMAL_TEXT", vec![run(&format!("{t}\n"), *b)])]})).collect();
    json!({"startIndex": at, "tableCells": cells})
}

#[test]
fn a_docs_api_document_is_read_by_its_styles_and_cells() {
    let doc = json!({"body": {"content": [
        para("HEADING_2", vec![run("Product codes for order export\n", true)]),
        para("NORMAL_TEXT", vec![run("Hardware ", true), run("line\n", true)]),
        {"table": {"tableRows": [row(100, &[("Product code", true), ("", false)]), row(120, &[("7", true), ("Alpha laptop", false)])]}},
        para("NORMAL_TEXT", vec![run("plain ", false), run("half bold\n", true)]),
        {"table": {"tableRows": [row(200, &[("20", true), ("Beta suite", false)])]}},
        para("HEADING_2", vec![run("Status\n", false)]),
    ]}});
    let blocks = blocks(std::path::Path::new("guide.json"), &doc.to_string());
    assert!(blocks.contains(&Block::Para { text: "plain half bold".into(), bold: false }));
    let found = section(&blocks, "Product codes for order export", 0).unwrap();
    let rows = table::rows(&found, &Column::Name("Product code".into()), &Some(Column::At(2))).unwrap();
    assert_eq!(
        keys(&rows),
        vec![("Hardware line".into(), "7".into(), "Alpha laptop".into()), ("Hardware line".into(), "20".into(), "Beta suite".into())]
    );
    assert_eq!(rows[0].at, 120);
}
