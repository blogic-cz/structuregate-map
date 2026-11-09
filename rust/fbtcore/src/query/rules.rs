//! `--rules [FRAGMENT]`: every rule of `crate::rules` and what it finds, whatever `structuregate.rules.json` switches -
//! what turning a rule on would report. One section per rule; FRAGMENT narrows to the matching paths.

use super::*;

pub(super) fn lens(db: &Connection, out: &mut String, fragment: &str, limit: usize, width: usize) -> Result<()> {
    let mut texts = std::collections::HashMap::new();
    for rule in crate::rules::all() {
        let found = crate::rules::findings(db, rule, fragment, &mut texts)?;
        let rows: Vec<Vec<Value>> = found
            .into_iter()
            .map(|f| vec![Value::Text(f.path), Value::Integer(f.line), Value::Text(f.name), Value::Text(f.kind)])
            .collect();
        let _ = writeln!(out, "{} - {} ({})", rule.id, rule.says, rows.len());
        show(out, &rows, &named(&["file", "line", "name", "type"]), limit, width);
        let _ = writeln!(out);
    }
    Ok(())
}
