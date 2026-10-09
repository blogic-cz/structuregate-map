//! WHAT A KEY BUILT FROM AN ENUM'S MEMBER NAME REQUIRES — the `tpl:` links `key_branches`
//! puts on a key a template literal spells.
//!
//! `` `labels.colors.${HueIDs[this.tier.HueID]}.Value` `` is
//! `labels.colors.Delta.Value` exactly when the id IS `Delta`: the enum's reverse
//! lookup turns the value into its member's name, and the name is the hole. So the link says
//! `tier.HueID in [Delta]`, read like any comparison.
//!
//! THE LOOKUP IS FOUND BY ITS TREE, never by the hole's text: a `KeyedRead` whose receiver
//! resolves to an enum declaration. A hole text that is no member of that enum is a way the
//! template cannot take, so it permits nothing (`in []`).
//!
//! A HOLE THAT IS THE ENUM VALUE ITSELF (`Feature_${item.FeatureID}`) prints the
//! member's VALUE, not its name: `Feature_9` needs the member whose value is 9. The same
//! rule prunes a match the pieces alone allow - `Feature_${id}` spelling
//! `Feature_9Tooltip` with the hole `9Tooltip` - since no member's value is that text.
//! Any other hole says nothing.

use super::gate_tsrows::TsRows;
use super::gate_values::{dimension_of, untyped_dimension, EnumIndex, MemberEnums};
use super::store::Store;
use indexmap::{IndexMap, IndexSet};
use serde_json::{json, Value};

/// Link id -> the restrictions its holes state, in `gate_values`' own map shape.
pub fn built_values(
    store: &Store<'_>,
    links: &IndexSet<String>,
    mem: &MemberEnums,
    idx: &EnumIndex,
) -> IndexMap<String, (Option<String>, Vec<Value>)> {
    let mut wanted: IndexMap<String, Vec<(String, Vec<String>)>> = IndexMap::new();
    for link in links {
        let Some((tree, holes)) = link.strip_prefix("tpl:").and_then(|l| l.split_once('=')) else { continue };
        let texts = holes.split('|').map(str::to_string).collect();
        wanted.entry(tree.to_string()).or_default().push((link.clone(), texts));
    }
    let mut out = IndexMap::new();
    if wanted.is_empty() {
        return out;
    }
    let rows = TsRows::new(store);
    let none = IndexSet::new();
    // Enum id -> value as printed -> member name, for a hole that interpolates the value.
    let mut printed: IndexMap<String, IndexMap<String, String>> = IndexMap::new();
    for e in store.table("enums").iter() {
        let Some(id) = e.get("id").and_then(|v| v.as_str()) else { continue };
        for m in e.get("members").and_then(|v| v.as_array()).into_iter().flatten() {
            let (Some(name), Some(value)) = (m.get("name").and_then(|v| v.as_str()), m.get("value")) else { continue };
            let text = match value {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                _ => continue,
            };
            printed.entry(id.to_string()).or_default().insert(text, name.to_string());
        }
    }
    for e in store.table("expressions").iter() {
        let Some(list) = e.get("id").and_then(|x| x.as_str()).and_then(|x| wanted.get(x)) else { continue };
        let Some(ast) = e.get("ast") else { continue };
        let tree = rows.resolve(super::astreads::unwrap(ast), &none, idx);
        let parts = tree.get("expressions").and_then(|x| x.as_array()).cloned().unwrap_or_default();
        for (link, texts) in list {
            let mut restrictions = Vec::new();
            for (hole, text) in parts.iter().zip(texts) {
                if let Some(r) = lookup(hole, text, mem, idx).or_else(|| printed_value(hole, text, mem, idx, &printed)) {
                    restrictions.push(r);
                }
            }
            if !restrictions.is_empty() {
                out.insert(link.clone(), (None, restrictions));
            }
        }
    }
    out
}

/// `Enum[x]` holding `text`, as the restriction on `x`.
fn lookup(hole: &Value, text: &str, mem: &MemberEnums, idx: &EnumIndex) -> Option<Value> {
    if hole.get("k").and_then(|k| k.as_str()) != Some("KeyedRead") {
        return None;
    }
    let en = hole.get("receiver")?.get("target")?.get("row")?.as_str()?;
    let domain = &idx.by_id.get(en)?.domain;
    let key = hole.get("key")?;
    let (dim, row) = match dimension_of(key, mem) {
        Some((typed, dim, row)) if typed != en => return Some(json!({"enum": typed, "dim": dim, "row": row, "op": "unknown"})),
        Some((_, dim, row)) => (dim, row),
        None => (untyped_dimension(key)?, None),
    };
    let values: Vec<&str> = if domain.contains(text) { vec![text] } else { Vec::new() };
    Some(json!({"enum": en, "dim": dim, "row": row, "op": "in", "values": values}))
}

/// A hole typed by an enum, printing `text`, as the restriction to the member that prints it.
fn printed_value(
    hole: &Value,
    text: &str,
    mem: &MemberEnums,
    idx: &EnumIndex,
    printed: &IndexMap<String, IndexMap<String, String>>,
) -> Option<Value> {
    let (en, dim, row) = dimension_of(hole, mem)?;
    idx.by_id.get(&en)?;
    let values: Vec<&String> = printed.get(&en).and_then(|m| m.get(text)).into_iter().collect();
    Some(json!({"enum": en, "dim": dim, "row": row, "op": "in", "values": values}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::gate_values::EnumInfo;

    fn idx() -> EnumIndex {
        let mut idx = EnumIndex::default();
        idx.by_id.insert("e:1".into(), EnumInfo {
            name: Some("VendorIDs".into()), 
            domain: ["Delta", "Beta", "Gamma"].iter().map(|s| s.to_string()).collect(),
        });
        idx
    }

    fn hole() -> Value {
        json!({"k": "KeyedRead",
               "receiver": {"k": "Read", "name": "VendorIDs", "receiver": {"k": "Implicit"}, "target": {"row": "e:1"}},
               "key": {"k": "Read", "name": "VendorID", "receiver": {"k": "Read", "name": "tier", "receiver": {"k": "This"}}}})
    }

    #[test]
    fn a_member_name_in_the_hole_is_that_member_of_the_key() {
        let r = lookup(&hole(), "Delta", &MemberEnums::default(), &idx()).expect("a restriction");
        assert_eq!((r["dim"].clone(), r["op"].clone(), r["values"].clone()), (json!("tier.VendorID"), json!("in"), json!(["Delta"])));
    }

    #[test]
    fn a_text_the_enum_has_no_member_of_is_a_way_that_permits_nothing() {
        let r = lookup(&hole(), "Omega", &MemberEnums::default(), &idx()).expect("a restriction");
        assert_eq!(r["values"], json!([]));
    }

    #[test]
    fn a_hole_printing_an_enum_value_is_the_member_with_that_value() {
        let mut mem = MemberEnums::default();
        mem.typed.insert("m:id".into(), "e:1".into());
        let hole = json!({"k": "Read", "name": "VendorID", "receiver": {"k": "This"}, "target": {"row": "m:id"}});
        let mut printed = IndexMap::new();
        printed.insert("e:1".to_string(), [("9".to_string(), "Delta".to_string())].into_iter().collect());
        let r = printed_value(&hole, "9", &mem, &idx(), &printed).expect("a restriction");
        assert_eq!(r["values"], json!(["Delta"]));
        let r = printed_value(&hole, "9Tooltip", &mem, &idx(), &printed).expect("a restriction");
        assert_eq!(r["values"], json!([]), "no member prints that, so the match is a way that cannot happen");
    }

    #[test]
    fn a_hole_that_is_no_enum_lookup_says_nothing() {
        let plain = json!({"k": "Read", "name": "tierName", "receiver": {"k": "Implicit"}});
        assert!(lookup(&plain, "Omega", &MemberEnums::default(), &idx()).is_none());
    }
}
