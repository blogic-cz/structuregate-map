//! A VALUE AS A SCRIPT SPELLS IT - `(kind, expr)`: `int`, `num`, `str` (an `N'...'`), `vstr` (a `'...'`),
//! `null`, `var`, `sum`, and `expr` / `unset` for one not known before run time - and the JSON the seed
//! rows carry it in.

/// `(kind, expr)`.
pub type Val = (String, String);

/// A number as .NET's `decimal.TryParse(NumberStyles.Number)` reads it: white space round it, a sign in
/// front (or behind), thousands separators in the whole part, one decimal point. `(negative, whole, fraction)`
/// with the whole part's leading zeros gone and the fraction AS WRITTEN - the scale is part of the value's
/// spelling, `1.50` stays `1.50`.
pub fn decimal(text: &str) -> Option<(bool, String, String)> {
    let mut s = text.trim();
    let mut negative = false;
    if let Some(rest) = s.strip_prefix('-') {
        negative = true;
        s = rest;
    } else if let Some(rest) = s.strip_prefix('+') {
        s = rest;
    } else if let Some(rest) = s.strip_suffix('-') {
        negative = true;
        s = rest;
    } else if let Some(rest) = s.strip_suffix('+') {
        s = rest;
    }
    let (whole, fraction) = match s.split_once('.') {
        Some((w, f)) => (w, f),
        None => (s, ""),
    };
    let whole: String = whole.chars().filter(|c| *c != ',').collect();
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    if !whole.chars().all(|c| c.is_ascii_digit()) || !fraction.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let whole = whole.trim_start_matches('0').to_string();
    Some((negative, whole, fraction.to_string()))
}

/// Two numbers by VALUE: `1.0` is `1`, and `-0` is `0`.
pub fn same_number(a: &str, b: &str) -> bool {
    let canon = |text: &str| {
        decimal(text).map(|(negative, whole, fraction)| {
            let fraction = fraction.trim_end_matches('0').to_string();
            let zero = whole.is_empty() && fraction.is_empty();
            (negative && !zero, whole, fraction)
        })
    };
    matches!((canon(a), canon(b)), (Some(x), Some(y)) if x == y)
}

/// How .NET prints a `decimal` it parsed from `text`: no leading zeros, the scale kept, no sign on zero.
pub fn decimal_text(text: &str) -> Option<String> {
    let (negative, whole, fraction) = decimal(text)?;
    let zero = whole.is_empty() && fraction.chars().all(|c| c == '0');
    let whole = if whole.is_empty() { "0".to_string() } else { whole };
    let sign = if negative && !zero { "-" } else { "" };
    Some(if fraction.is_empty() { format!("{sign}{whole}") } else { format!("{sign}{whole}.{fraction}") })
}

/// A JSON string, escaped by `serde_json` - which writes the text as it is and escapes only a quote, a
/// backslash and the control characters. So `Crème brûlée` and a no-break space stay what the script spells,
/// and a LIKE over the column finds them. (The C# pass this replaced wrote .NET's relaxed escaping, which spells
/// a no-break space ` `; every row holds the same JSON VALUES either way - thousands of rows compared.)
pub fn json_string(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into())
}
/// A list of names as a JSON array - the text a seed is told apart by (`walk.rs`), never a stored cell.
pub fn columns(names: &[String]) -> String {
    format!("[{}]", names.iter().map(|n| json_string(n)).collect::<Vec<_>>().join(","))
}

/// The values as JSON text - `walk.rs`'s key, never a stored cell: a number, a string or null as SQL wrote it, and one not known before run time as its
/// own text.
pub fn values(values: &[Val]) -> String {
    let cells: Vec<String> = values
        .iter()
        .map(|(kind, expr)| match kind.as_str() {
            "null" => "null".to_string(),
            "int" if expr.parse::<i64>().is_ok() => expr.parse::<i64>().unwrap().to_string(),
            "num" => decimal_text(expr).unwrap_or_else(|| json_string(expr)),
            _ => json_string(expr),
        })
        .collect();
    format!("[{}]", cells.join(","))
}

/// The values as the list `sql_seeds.row_values` holds: a number, a string or null as SQL wrote it, and one not
/// known before run time as its own text. A `num` is the number it reads as (`1.50` is 1.5 - the scale is in
/// `text`); one too large for that stays its text.
pub fn listed(values: &[Val]) -> serde_json::Value {
    let cells = values.iter().map(|(kind, expr)| match kind.as_str() {
        "null" => serde_json::Value::Null,
        "int" if expr.parse::<i64>().is_ok() => serde_json::Value::from(expr.parse::<i64>().unwrap_or(0)),
        "num" => match decimal_text(expr) {
            Some(text) => match (text.parse::<i64>(), text.parse::<f64>()) {
                (Ok(n), _) => serde_json::Value::from(n),
                (_, Ok(f)) if f.is_finite() => serde_json::Value::from(f),
                _ => serde_json::Value::String(text),
            },
            None => serde_json::Value::String(expr.clone()),
        },
        _ => serde_json::Value::String(expr.clone()),
    });
    serde_json::Value::Array(cells.collect())
}

/// A value as a reader of a message sees it.
pub fn shown((kind, expr): &Val) -> String {
    match kind.as_str() {
        "null" => "NULL".into(),
        "str" => format!("N'{expr}'"),
        "vstr" => format!("'{expr}'"),
        _ => expr.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_is_equal_by_value_and_printed_with_its_scale() {
        assert!(same_number("1.0", "1") && same_number("-0", "0") && same_number("1,000", "1000"));
        assert!(!same_number("1.5", "15") && !same_number("x", "x"));
        assert_eq!(decimal_text("007.50").as_deref(), Some("7.50"));
        assert_eq!(decimal_text("-0.0").as_deref(), Some("0.0"));
        assert_eq!(values(&[("int".into(), "5".into()), ("str".into(), "Crème \"x\"".into()), ("null".into(), "".into())]),
            r#"[5,"Crème \"x\"",null]"#);
        assert_eq!(
            listed(&[("int".into(), "5".into()), ("num".into(), "007.50".into()), ("str".into(), "x".into()), ("null".into(), "".into())]),
            serde_json::json!([5, 7.5, "x", null])
        );
    }
}
