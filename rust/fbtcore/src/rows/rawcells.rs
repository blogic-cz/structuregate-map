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
use serde::Deserializer;
#[cfg(test)]
use serde::Deserialize;
use serde_json::value::RawValue;
use serde_json::{Map, Value};

/// The key of the wrapper. A NUL cannot start a key node writes, so no real cell is mistaken for one.
const RAW: &str = "\u{0}raw";

/// `TsPayload.tables`, with every list and object cell kept as its text - a FIXTURE'S door: a run streams them
/// (`each_row`).
#[cfg(test)]
pub fn tables<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Map<String, Value>, D::Error> {
    let raw: IndexMap<String, Box<RawValue>> = IndexMap::deserialize(deserializer)?;
    let mut out = Map::new();
    for (name, table) in raw {
        let rows = match rows_of(table.get()) {
            Ok(rows) => Value::Array(rows.into_iter().map(|row| Value::Object(row.into_iter().map(|(k, v)| (k, cell(&v))).collect())).collect()),
            // Not a list of rows: kept as it was, decoded.
            Err(_) => super::cells::deep(table.get()).map_err(serde::de::Error::custom)?,
        };
        out.insert(name, rows);
    }
    Ok(out)
}

/// A table's rows, each cell still its text - with no depth limit, as every read of a tree here.
#[cfg(test)]
fn rows_of(text: &str) -> serde_json::Result<Vec<IndexMap<String, Box<RawValue>>>> {
    let mut reader = serde_json::Deserializer::from_str(text);
    reader.disable_recursion_limit();
    let rows = Vec::<IndexMap<String, Box<RawValue>>>::deserialize(&mut reader)?;
    reader.end()?;
    Ok(rows)
}

fn cell(raw: &RawValue) -> Value {
    let text = raw.get();
    match text.as_bytes().first() {
        Some(b'{') | Some(b'[') => Value::Object(Map::from_iter([(RAW.to_string(), Value::String(text.to_string()))])),
        _ => serde_json::from_str(text).unwrap_or(Value::Null),
    }
}

/// EVERY ROW OF A PAYLOAD'S `tables`, handed to `row` one at a time and never held: `{"tables": {"<table>": [rows]}}`,
/// every other field skipped. A row's list and object cells stay their text (`cell`), as `tables` keeps them. No
/// depth limit. A table that is not a list of rows is skipped - nothing stores one.
///
/// THE PAYLOAD IS NEVER PARSED WHOLE: a large workspace's is gigabytes of rows once parsed, and holding them to
/// write them was most of this half's peak memory. Called twice by a run - once for what the rows say about each
/// other (`partial::digest`), once while they are written.
pub fn each_row(bytes: &[u8], row: &mut dyn FnMut(&str, Map<String, Value>) -> anyhow::Result<()>) -> anyhow::Result<()> {
    use serde::de::{DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};
    use std::fmt;

    /// The error a row callback raised, carried out through serde and handed back as itself.
    struct Failed<'f>(&'f mut Option<anyhow::Error>);
    struct Top<'r, 'f>(&'r mut dyn FnMut(&str, Map<String, Value>) -> anyhow::Result<()>, Failed<'f>);
    struct Tables<'r, 'f>(&'r mut dyn FnMut(&str, Map<String, Value>) -> anyhow::Result<()>, &'f mut Option<anyhow::Error>);
    struct Rows<'r, 'f, 't>(&'r mut dyn FnMut(&str, Map<String, Value>) -> anyhow::Result<()>, &'f mut Option<anyhow::Error>, &'t str);

    impl<'de> Visitor<'de> for Top<'_, '_> {
        type Value = ();
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a payload object")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
            while let Some(key) = map.next_key::<String>()? {
                if key == "tables" {
                    map.next_value_seed(Tables(&mut *self.0, &mut *self.1 .0))?;
                } else {
                    map.next_value::<IgnoredAny>()?;
                }
            }
            Ok(())
        }
    }
    impl<'de> DeserializeSeed<'de> for Top<'_, '_> {
        type Value = ();
        fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
            d.deserialize_map(self)
        }
    }
    impl<'de> Visitor<'de> for Tables<'_, '_> {
        type Value = ();
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("an object of tables")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
            while let Some(table) = map.next_key::<String>()? {
                map.next_value_seed(Rows(&mut *self.0, &mut *self.1, &table))?;
            }
            Ok(())
        }
    }
    impl<'de> DeserializeSeed<'de> for Tables<'_, '_> {
        type Value = ();
        fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
            d.deserialize_map(self)
        }
    }
    impl<'de> Visitor<'de> for Rows<'_, '_, '_> {
        type Value = ();
        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("a list of rows")
        }
        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
            while let Some(raw) = seq.next_element::<IndexMap<String, Box<RawValue>>>()? {
                let row: Map<String, Value> = raw.into_iter().map(|(k, v)| (k, cell(&v))).collect();
                if let Err(e) = (self.0)(self.2, row) {
                    let shown = format!("{e:#}");
                    *self.1 = Some(e);
                    return Err(serde::de::Error::custom(shown));
                }
            }
            Ok(())
        }
        // Not a list of rows: nothing stores one, so it is read past.
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
            while map.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
            Ok(())
        }
        fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<(), E> {
            Ok(())
        }
        fn visit_unit<E: serde::de::Error>(self) -> Result<(), E> {
            Ok(())
        }
        fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<(), E> {
            Ok(())
        }
        fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<(), E> {
            Ok(())
        }
        fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<(), E> {
            Ok(())
        }
        fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<(), E> {
            Ok(())
        }
    }
    impl<'de> DeserializeSeed<'de> for Rows<'_, '_, '_> {
        type Value = ();
        fn deserialize<D: Deserializer<'de>>(self, d: D) -> Result<(), D::Error> {
            d.deserialize_any(self)
        }
    }

    let mut failed: Option<anyhow::Error> = None;
    let mut reader = serde_json::Deserializer::from_slice(bytes);
    reader.disable_recursion_limit();
    let walked = Top(row, Failed(&mut failed)).deserialize(&mut reader).and_then(|_| reader.end());
    if let Some(e) = failed {
        return Err(e);
    }
    walked.map_err(|e| anyhow::anyhow!("the rows could not be read - {e}"))
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

#[cfg(test)]
mod streamed {
    use super::*;

    #[test]
    fn every_row_is_handed_over_in_order_and_nothing_else_is() {
        let payload = br#"{"all": true, "tables": {"files": [{"id": "f:1", "reads": ["a"]}], "odd": {"x": 1},
                         "calls": [{"id": "c:1", "args": [1, [2]]}, {"id": "c:2"}]}, "shas": {"a": "1"}}"#;
        let mut seen: Vec<(String, String, Option<String>)> = Vec::new();
        each_row(payload, &mut |table, row| {
            let raw = row.get("reads").or_else(|| row.get("args")).and_then(text_of).map(String::from);
            seen.push((table.to_string(), row["id"].as_str().unwrap().to_string(), raw));
            Ok(())
        })
        .unwrap();
        assert_eq!(seen, [
            ("files".to_string(), "f:1".to_string(), Some(r#"["a"]"#.to_string())),
            ("calls".to_string(), "c:1".to_string(), Some("[1, [2]]".to_string())),
            ("calls".to_string(), "c:2".to_string(), None),
        ]);
    }

    #[test]
    fn a_row_the_caller_refuses_stops_the_walk_with_the_callers_own_error() {
        let payload = br#"{"tables": {"files": [{"id": "f:1"}, {"id": "f:2"}]}}"#;
        let mut count = 0;
        let error = each_row(payload, &mut |_, _| {
            count += 1;
            anyhow::bail!("refused here")
        })
        .unwrap_err();
        assert_eq!(count, 1);
        assert_eq!(format!("{error}"), "refused here");
    }
}
