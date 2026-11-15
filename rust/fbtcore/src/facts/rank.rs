//! HOW MUCH A `--facts-suggest` PAIR MEANS, beyond how many values it shares. Two things a count alone gets wrong:
//!
//! - CHANCE: a column of `1..50` shares every value with any set of small integers. How many hits a set gives by chance
//!   is how densely it covers the DOCUMENT'S OWN RANGE of integers: a set holding every `1..N` matches a `1..50` column
//!   fully, whatever large values it also holds. That many hits say nothing; they are taken off.
//! - ROWS THAT AGREE: the true link is the one where the rest of the row agrees too - the document's label column
//!   names the same seed row's other column, or the enum member's name. A pair with agreeing rows ranks above every
//!   pair without, whatever the counts.

use super::links::{self, Code};
use std::collections::HashMap;

/// A pair's measure: its hits, the hits chance alone gives, and the rows whose other cells agree (with the document
/// column - from 1 - and the code column that agree most).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Score {
    pub hits: usize,
    pub chance: f64,
    pub agree: usize,
    pub doc_column: usize,
    pub code_column: String,
}

impl Score {
    /// Higher first: agreeing rows, then hits beyond chance, then hits.
    pub fn rank(&self) -> (usize, i64, usize) {
        (self.agree, ((self.hits as f64 - self.chance) * 100.0) as i64, self.hits)
    }
}

/// The document's rows a table column belongs to: every data row, the column's place in it, the header.
pub struct Table {
    pub rows: Vec<Vec<String>>,
    pub column: usize,
}

/// How many document integers a set would hold anyway: the document's integers times how densely the set covers
/// their range, `lo..=hi`. A value of the set outside that range is no evidence either way.
pub fn chance(document: &[String], codes: &[Code]) -> f64 {
    let ints = |values: &mut dyn Iterator<Item = &str>| values.filter_map(|v| v.trim().parse::<i64>().ok()).collect::<std::collections::HashSet<i64>>();
    let ours = ints(&mut document.iter().map(String::as_str));
    let (Some(&lo), Some(&hi)) = (ours.iter().min(), ours.iter().max()) else { return 0.0 };
    let theirs = ints(&mut codes.iter().map(|c| c.value.as_str()));
    let covered = theirs.iter().filter(|n| (lo..=hi).contains(*n)).count();
    ours.len() as f64 * covered as f64 / (hi - lo + 1) as f64
}

/// The rows of a table whose key matches a code value AND whose other cell names that code row's other value: per
/// (document column, code column), the most rows. `others` is each code row's other columns, by its `target`; a code
/// with none has its `name` (an enum member's, a constant's).
pub fn agreement(table: &Table, kind: &str, key_column: &str, codes: &[Code], others: &HashMap<String, Vec<(String, String)>>) -> (usize, usize, String) {
    let mut by_value: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, c) in codes.iter().enumerate() {
        by_value.entry(normal(&c.value)).or_default().push(i);
    }
    let mut tally: HashMap<(usize, String), usize> = HashMap::new();
    for row in &table.rows {
        let Some(key) = row.get(table.column).filter(|k| !k.trim().is_empty()) else { continue };
        let hits: Vec<&Code> = by_value.get(&normal(key)).into_iter().flatten().map(|&i| &codes[i]).filter(|c| links::matches(kind, &c.value, key)).collect();
        let mut counted: std::collections::HashSet<(usize, String)> = std::collections::HashSet::new();
        for (j, cell) in row.iter().enumerate().filter(|(j, cell)| *j != table.column && !cell.trim().is_empty()) {
            for code in &hits {
                let named = (!code.name.is_empty() && code.name != code.value).then(|| vec![("name".to_string(), code.name.clone())]);
                let theirs = others.get(&code.target).cloned().or(named).unwrap_or_default();
                for (column, value) in theirs.iter().filter(|(column, _)| !column.eq_ignore_ascii_case(key_column)) {
                    if same_cell(value, cell) && counted.insert((j, column.clone())) {
                        *tally.entry((j, column.clone())).or_default() += 1;
                    }
                }
            }
        }
    }
    tally.into_iter().max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0))).map(|((j, column), n)| (n, j + 1, column)).unwrap_or_default()
}

fn normal(value: &str) -> String {
    let value = value.trim();
    value.parse::<f64>().map(|n| n.to_string()).unwrap_or_else(|_| value.to_lowercase())
}

fn same_cell(code: &str, cell: &str) -> bool {
    links::same(code, cell) || code.trim().eq_ignore_ascii_case(cell.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code(target: &str, value: &str, name: &str) -> Code {
        Code { target: target.into(), value: value.into(), name: name.into(), ..Default::default() }
    }

    #[test]
    fn a_dense_small_integer_set_is_expected_to_match_and_a_sparse_one_is_not() {
        let document: Vec<String> = (1..=50).map(|n| n.to_string()).collect();
        let dense: Vec<Code> = (1..=30).map(|n| code("e", &n.to_string(), "")).collect();
        assert_eq!(chance(&document, &dense), 30.0);
        let sparse: Vec<Code> = [7, 300, 9000].iter().map(|n| code("s", &n.to_string(), "")).collect();
        assert_eq!(chance(&document, &sparse), 1.0);
        // EVERY 1..N AND A FEW LARGE VALUES: the large ones say nothing about the document's range.
        let ordinal: Vec<Code> = (1..=60).chain([5000, 9000]).map(|n| code("o", &n.to_string(), "")).collect();
        assert_eq!(chance(&document, &ordinal), 50.0);
        assert_eq!(chance(&["Alpha".to_string()], &dense), 0.0);
    }

    #[test]
    fn rows_agree_when_the_label_names_the_same_seed_row_or_the_members_name() {
        // The third column repeats the key: the seed's own key column agreeing with it is no agreement.
        let row = |k: &str, n: &str| vec![k.to_string(), n.to_string(), k.to_string()];
        let table = Table { rows: vec![row("1", "Pen"), row("2", "Cup"), row("3", "Bag")], column: 0 };
        let seed = vec![code("sd:1", "1", ""), code("sd:2", "2", ""), code("sd:3", "3", "")];
        let others = HashMap::from([
            ("sd:1".to_string(), vec![("ItemNo".to_string(), "1".to_string()), ("Name".to_string(), "Pen".to_string())]),
            ("sd:2".to_string(), vec![("ItemNo".to_string(), "2".to_string()), ("Name".to_string(), "Cup".to_string())]),
            ("sd:3".to_string(), vec![("ItemNo".to_string(), "3".to_string()), ("Name".to_string(), "Hat".to_string())]),
        ]);
        assert_eq!(agreement(&table, "seed", "ItemNo", &seed, &others), (2, 2, "Name".to_string()));
        let members = vec![code("e:1", "1", "Gold"), code("e:1", "2", "Cup"), code("e:1", "3", "Silver")];
        assert_eq!(agreement(&table, "enum", "", &members, &HashMap::new()), (1, 2, "name".to_string()));
        let scored = |agree, hits, chance| Score { hits, chance, agree, ..Default::default() }.rank();
        assert!(scored(2, 45, 50.0) > scored(0, 50, 30.0));
        assert!(scored(0, 20, 0.0) > scored(0, 50, 50.0));
    }
}
