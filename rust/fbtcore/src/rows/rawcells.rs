//! A PAYLOAD'S LISTS AND OBJECTS STAY THE TEXT NODE SENT until the moment they are written.
//!
//! A cell that holds a list or an object - above all `expressions.ast` - is stored as JSON text, so on a partial
//! run it is decoded only to be spelled back out: a large Angular workspace's affected rows became more than twenty times their size in
//! value trees in between. So such a cell is kept as its raw text, wrapped in a one-key object only this module
//! makes, and `schema::cell` turns it into python's spelling at the write - the same parse and the same `dumps`
//! as before, one cell at a time instead of every cell at once. The stored bytes cannot differ.
//!
//! THE CLOSURE READS THE ROWS BACK from the database once they are written (`rows/tsapply.rs`), and its store
//! decodes a cell the first time a pass reads it (`rows/ts/store.rs`, `Cell`).

use indexmap::IndexMap;
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use serde_json::{Map, Value};

/// The key of the wrapper. A NUL cannot start a key node writes, so no real cell is mistaken for one.
const RAW: &str = "\u{0}raw";

/// `TsPayload.tables`, with every list and object cell kept as its text.
pub fn tables<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Map<String, Value>, D::Error> {
    let raw: IndexMap<String, Box<RawValue>> = IndexMap::deserialize(deserializer)?;
    let mut out = Map::new();
    for (name, table) in raw {
        let rows = match serde_json::from_str::<Vec<IndexMap<String, Box<RawValue>>>>(table.get()) {
            Ok(rows) => Value::Array(rows.into_iter().map(|row| Value::Object(row.into_iter().map(|(k, v)| (k, cell(&v))).collect())).collect()),
            // Not a list of rows: kept as it was, decoded.
            Err(_) => serde_json::from_str(table.get()).map_err(serde::de::Error::custom)?,
        };
        out.insert(name, rows);
    }
    Ok(out)
}

fn cell(raw: &RawValue) -> Value {
    let text = raw.get();
    match text.as_bytes().first() {
        Some(b'{') | Some(b'[') => Value::Object(Map::from_iter([(RAW.to_string(), Value::String(text.to_string()))])),
        _ => serde_json::from_str(text).unwrap_or(Value::Null),
    }
}

/// A cell kept as its text, wrapped the way `tables` wraps one.
pub fn wrap(text: String) -> Value {
    Value::Object(Map::from_iter([(RAW.to_string(), Value::String(text))]))
}

/// The text a wrapped cell holds, or None for any other value.
pub fn text_of(value: &Value) -> Option<&str> {
    match value {
        Value::Object(map) if map.len() == 1 => map.get(RAW).and_then(Value::as_str),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Holder {
        #[serde(deserialize_with = "tables")]
        tables: Map<String, Value>,
    }

    #[test]
    fn a_list_or_object_cell_is_kept_as_its_text_and_spelled_at_the_write_exactly_as_before() {
        let payload = r#"{"tables":{"expressions":[{"id":"x:1","ast":{"k":"Read","name":"a"},"reads":["a","b"],"line":3}]}}"#;
        let held: Holder = serde_json::from_reader(payload.as_bytes()).unwrap();
        let row = &held.tables["expressions"][0];
        assert_eq!(text_of(&row["ast"]), Some(r#"{"k":"Read","name":"a"}"#));
        assert_eq!(row["line"], Value::from(3));
        assert_eq!(row["id"], Value::from("x:1"));
        let decoded: Value = serde_json::from_str(r#"{"k":"Read","name":"a"}"#).unwrap();
        assert_eq!(super::super::pyjson::dumps(&decoded), r#"{"k": "Read", "name": "a"}"#);
        assert_eq!(text_of(&held.tables["expressions"][0]["reads"]), Some(r#"["a","b"]"#));
        assert_eq!(text_of(&wrap("[1]".into())), Some("[1]"));
    }
}
